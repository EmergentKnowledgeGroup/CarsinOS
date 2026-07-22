#![cfg_attr(not(test), allow(dead_code))]

//! Immutable, source-referenced authority lineage for the ExecAss aggregate.
//!
//! Legacy CarsinOS sources do not share a safe monotonic revision.  Therefore
//! every currently supported source is observed at authoritative revision zero
//! and is re-read by exact primary key during resolution.  `updated_at` is not
//! a version and is never promoted into authority proof.

use super::rows::{get_delegation, get_outbox, insert_outbox};
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::{require_text, validate_lineage_command};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

impl ExecAssStore {
    pub(super) fn append_authority_lineage(
        &self,
        command: &AppendAuthorityLineageCommand,
    ) -> Result<AuthorityLineageOutcome> {
        self.append_authority_lineage_internal(command, &[], false)
    }

    pub(super) fn append_authority_lineage_with_ownership(
        &self,
        command: &AppendAuthorityLineageCommand,
        ownership_checks: &[AuthorityOwnershipCheck],
    ) -> Result<AuthorityLineageOutcome> {
        self.append_authority_lineage_internal(command, ownership_checks, true)
    }

    fn append_authority_lineage_internal(
        &self,
        command: &AppendAuthorityLineageCommand,
        ownership_checks: &[AuthorityOwnershipCheck],
        enforce_ownership: bool,
    ) -> Result<AuthorityLineageOutcome> {
        validate_lineage_command(command)?;
        if enforce_ownership {
            validate_ownership_checks(&command.links, ownership_checks)?;
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;

        // Ownership is part of the adapter observation, so even an idempotent
        // replay must re-read it before returning the immutable prior result.
        for link in &command.links {
            if let Some(check) = ownership_checks
                .iter()
                .find(|check| check.link_id == link.link_id)
            {
                resolve_target(&tx, &link.target)?;
                if let Some((expected_owner, actual_owner)) = ownership_mismatch(&tx, link, check)?
                {
                    return Ok(AuthorityLineageOutcome::OwnershipMismatch {
                        kind: link.target.kind().context("unsupported authority target")?,
                        source_id: link
                            .target
                            .source_id()
                            .context("unsupported authority target")?
                            .into(),
                        expected_owner,
                        actual_owner,
                    });
                }
            }
        }

        if let Some(existing) =
            outbox_by_duplicate_identity(&tx, &command.outbox_event.duplicate_identity)?
        {
            let links = resolve_links_for_event(&tx, &existing.event.event_id)?;
            if existing.event == command.outbox_event
                && existing.event.aggregate_id == command.delegation_id
                && canonical_members(&links) == canonical_requested_members(&command.links)?
                && persisted_parent_bindings(&tx, &existing.event.event_id)?
                    == canonical_parent_bindings(ownership_checks)
            {
                tx.commit().context("failed closing lineage replay")?;
                return Ok(AuthorityLineageOutcome::Replayed(AuthorityLineageAppend {
                    delegation_id: existing.event.aggregate_id.clone(),
                    resulting_state_revision: existing.event.aggregate_revision,
                    outbox_event: existing,
                    links,
                }));
            }
            return Ok(AuthorityLineageOutcome::Conflict {
                duplicate_identity: command.outbox_event.duplicate_identity.clone(),
            });
        }

        let Some(current) = get_delegation(&tx, &command.delegation_id)? else {
            return Ok(AuthorityLineageOutcome::NotFound);
        };
        if current.state_revision != command.expected_state_revision {
            return Ok(AuthorityLineageOutcome::Stale {
                current_state_revision: current.state_revision,
            });
        }

        // Re-read every cited source before changing the delegation. This is a
        // reachability check only; no mutable source content is copied.
        for link in &command.links {
            resolve_target(&tx, &link.target)?;
        }

        let changed = tx.execute(
            "UPDATE execass_delegations SET state_revision = ?1, updated_at = ?2 WHERE delegation_id = ?3 AND state_revision = ?4",
            params![command.resulting_state_revision, command.linked_at, command.delegation_id, command.expected_state_revision],
        ).context("failed advancing delegation for authority lineage")?;
        if changed != 1 {
            bail!("delegation disappeared during IMMEDIATE authority lineage CAS");
        }
        insert_outbox(&tx, &command.outbox_event)?;
        for link in &command.links {
            insert_link(&tx, command, link)?;
            if let Some(check) = ownership_checks
                .iter()
                .find(|check| check.link_id == link.link_id)
            {
                tx.execute(
                    "INSERT INTO execass_authority_parent_bindings (link_id,owner_kind,expected_owner_id) VALUES (?1,?2,?3)",
                    params![check.link_id, owner_kind_str(check.owner_kind), check.expected_owner_id],
                )
                .context("failed inserting immutable authority parent binding")?;
            }
        }
        let delegation = get_delegation(&tx, &command.delegation_id)?
            .context("authority lineage delegation disappeared after append")?;
        let outbox_event = get_outbox(&tx, &command.outbox_event.event_id)?
            .context("authority lineage outbox event disappeared after append")?;
        if delegation.state_revision != command.resulting_state_revision
            || outbox_event.event != command.outbox_event
        {
            bail!("authority lineage post-commit verification mismatch");
        }
        let links = resolve_links_for_event(&tx, &command.outbox_event.event_id)?;
        if canonical_members(&links) != canonical_requested_members(&command.links)? {
            bail!("authority lineage persisted members do not match canonical request");
        }
        if persisted_parent_bindings(&tx, &command.outbox_event.event_id)?
            != canonical_parent_bindings(ownership_checks)
        {
            bail!("authority lineage persisted parent bindings do not match canonical request");
        }
        tx.commit()
            .context("failed committing immutable authority lineage")?;
        Ok(AuthorityLineageOutcome::Appended(AuthorityLineageAppend {
            delegation_id: command.delegation_id.clone(),
            resulting_state_revision: command.resulting_state_revision,
            outbox_event,
            links,
        }))
    }

    pub fn resolve_authority_lineage(
        &self,
        delegation_id: &str,
    ) -> Result<Vec<AuthorityLinkProjection>> {
        require_text("delegation_id", delegation_id)?;
        let conn = self.connection()?;
        resolve_links_for_delegation(&conn, delegation_id)
    }

    pub(super) fn resolve_authority_parent_drift(
        &self,
        delegation_id: &str,
    ) -> Result<Option<AuthorityOwnershipMismatch>> {
        let conn = self.connection()?;
        conn.query_row(
            r#"SELECT authority_kind,source_id,
                      COALESCE(expected_owner_id,'persisted authoritative parent binding'),
                      actual_owner_id
               FROM (
                 SELECT l.link_revision,l.authority_kind,
                   COALESCE(l.session_id,l.run_id,l.job_id,l.job_run_id,l.task_id,
                            l.board_card_id,l.mail_message_id,l.attachment_id,
                            l.board_card_asset_id,l.mail_attachment_id,
                            l.assistant_tool_call_audit_event_id,l.tool_call_id) AS source_id,
                   b.expected_owner_id,
                   CASE l.authority_kind
                     WHEN 'session' THEN (SELECT agent_id FROM sessions WHERE session_id=l.session_id)
                     WHEN 'run' THEN (SELECT session_id FROM runs WHERE run_id=l.run_id)
                     WHEN 'job' THEN (SELECT agent_id FROM jobs WHERE job_id=l.job_id)
                     WHEN 'job_run' THEN (SELECT job_id FROM job_runs WHERE job_run_id=l.job_run_id)
                     WHEN 'task' THEN (SELECT project_id FROM tasks WHERE task_id=l.task_id)
                     WHEN 'board_card' THEN (SELECT board_id FROM board_cards WHERE card_id=l.board_card_id)
                     WHEN 'mail_message' THEN (SELECT thread_id FROM agent_mail_messages WHERE message_id=l.mail_message_id)
                     WHEN 'artifact_attachment' THEN (SELECT message_id FROM attachments WHERE attachment_id=l.attachment_id)
                     WHEN 'artifact_board_card_asset' THEN (SELECT card_id FROM board_card_assets WHERE card_asset_id=l.board_card_asset_id)
                     WHEN 'artifact_mail_attachment' THEN (SELECT message_id FROM agent_mail_attachments WHERE attachment_id=l.mail_attachment_id)
                     WHEN 'assistant_tool_call_audit' THEN (SELECT root_session_id FROM assistant_tool_calls_audit WHERE event_id=l.assistant_tool_call_audit_event_id)
                     WHEN 'tool_call' THEN (SELECT run_id FROM tool_calls WHERE tool_call_id=l.tool_call_id)
                   END AS actual_owner_id
                 FROM execass_authority_links l
                 LEFT JOIN execass_authority_parent_bindings b ON b.link_id=l.link_id
                 WHERE l.delegation_id=?1 AND l.authority_kind IN (
                   'session','run','job','job_run','task','board_card','mail_message',
                   'artifact_attachment','artifact_board_card_asset','artifact_mail_attachment',
                   'assistant_tool_call_audit','tool_call'
                 )
               ) WHERE expected_owner_id IS NULL OR actual_owner_id IS NOT expected_owner_id
               ORDER BY link_revision LIMIT 1"#,
            params![delegation_id],
            |row| {
                Ok(AuthorityOwnershipMismatch {
                    kind: row.get(0)?,
                    source_id: row.get(1)?,
                    expected_owner: row.get(2)?,
                    actual_owner: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Structural graph validation only. It deliberately does not interpret
    /// lifecycle legality or receipt cryptography.
    pub fn validate_delegation_reachability(
        &self,
        delegation_id: &str,
    ) -> Result<DelegationReachabilityOutcome> {
        require_text("delegation_id", delegation_id)?;
        let conn = self.connection()?;
        let Some(delegation) = get_delegation(&conn, delegation_id)? else {
            return Ok(DelegationReachabilityOutcome::NotFound);
        };
        let checks = [
            ("delegation", "SELECT 'delegation:'||delegation_id||':authority' FROM execass_delegations d WHERE delegation_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_authority_provenance a WHERE a.authority_provenance_id=d.authority_provenance_id)"),
            ("delegation_transition", "SELECT 'delegation:'||d.delegation_id||':transition_revision_set' FROM execass_delegations d WHERE d.delegation_id=?1 AND ((SELECT COUNT(*) FROM execass_outbox_events o WHERE o.aggregate_id=d.delegation_id AND o.event_name='execass.v1.delegation.transitioned' AND o.aggregate_revision BETWEEN 1 AND d.state_revision) != d.state_revision OR EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.aggregate_id=d.delegation_id AND o.event_name='execass.v1.delegation.transitioned' AND o.aggregate_revision>d.state_revision))"),
            ("plan", "SELECT 'plan:'||plan_id||':authority_or_revision' FROM execass_plans p WHERE delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_authority_provenance a WHERE a.authority_provenance_id=p.created_by_authority_provenance_id) OR NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.aggregate_id=p.delegation_id AND o.aggregate_revision=p.based_on_delegation_revision AND o.event_name='execass.v1.delegation.transitioned') OR p.plan_revision != (SELECT COUNT(*) FROM execass_plans x WHERE x.delegation_id=p.delegation_id AND x.plan_revision<=p.plan_revision))"),
            ("amendment", "SELECT 'amendment:'||amendment_id||':plan_authority_or_revision' FROM execass_plan_amendments a WHERE delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_plans p WHERE p.delegation_id=a.delegation_id AND p.plan_revision=a.superseded_plan_revision) OR NOT EXISTS (SELECT 1 FROM execass_plans p WHERE p.delegation_id=a.delegation_id AND p.plan_revision=a.resulting_plan_revision) OR NOT EXISTS (SELECT 1 FROM execass_authority_provenance p WHERE p.authority_provenance_id=a.authority_provenance_id) OR a.resulting_plan_revision != a.superseded_plan_revision+1 OR a.amendment_revision != (SELECT COUNT(*) FROM execass_plan_amendments x WHERE x.delegation_id=a.delegation_id AND x.amendment_revision<=a.amendment_revision))"),
            ("criteria", "SELECT 'criterion:'||criterion_id||':revision' FROM execass_outcome_criteria c WHERE delegation_id=?1 AND (c.criteria_revision > COALESCE((SELECT current_criteria_revision FROM execass_delegations d WHERE d.delegation_id=c.delegation_id),0) OR c.criteria_revision != (SELECT COUNT(DISTINCT x.criteria_revision) FROM execass_outcome_criteria x WHERE x.delegation_id=c.delegation_id AND x.criteria_revision<=c.criteria_revision))"),
            ("verifier", "SELECT 'verifier:'||verifier_result_id||':criterion_or_revision' FROM execass_verifier_results v WHERE delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_outcome_criteria c WHERE c.delegation_id=v.delegation_id AND c.criterion_id=v.criterion_id) OR v.result_revision != (SELECT COUNT(*) FROM execass_verifier_results x WHERE x.criterion_id=v.criterion_id AND x.result_revision<=v.result_revision))"),
            ("decision", "SELECT 'decision:'||decision_id||':plan_authority_or_revision' FROM execass_decisions d WHERE delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_plans p WHERE p.delegation_id=d.delegation_id AND p.plan_revision=d.plan_revision) OR NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.aggregate_id=d.delegation_id AND o.aggregate_revision=d.delegation_revision AND o.event_name='execass.v1.delegation.transitioned') OR d.decision_revision != (SELECT COUNT(*) FROM execass_decisions x WHERE x.delegation_id=d.delegation_id AND x.decision_revision<=d.decision_revision) OR (d.resolved_by_authority_provenance_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM execass_authority_provenance a WHERE a.authority_provenance_id=d.resolved_by_authority_provenance_id)))"),
            ("continuation", "SELECT 'continuation:'||continuation_id||':plan_revision_or_causation' FROM execass_continuations c WHERE delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_plans p WHERE p.delegation_id=c.delegation_id AND p.plan_revision=c.target_plan_revision) OR NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.aggregate_id=c.delegation_id AND o.aggregate_revision=c.target_delegation_revision AND o.event_name='execass.v1.delegation.transitioned') OR NOT ((c.causation_kind='intake' AND EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.aggregate_id=c.delegation_id AND o.aggregate_revision=c.target_delegation_revision AND o.causation_id=c.causation_id)) OR (c.causation_kind='plan' AND EXISTS (SELECT 1 FROM execass_plans p WHERE p.delegation_id=c.delegation_id AND p.plan_id=c.causation_id AND p.plan_revision=c.target_plan_revision)) OR (c.causation_kind='amendment' AND EXISTS (SELECT 1 FROM execass_plan_amendments a WHERE a.delegation_id=c.delegation_id AND a.amendment_id=c.causation_id AND a.resulting_plan_revision=c.target_plan_revision)) OR (c.causation_kind='decision' AND EXISTS (SELECT 1 FROM execass_decisions d WHERE d.delegation_id=c.delegation_id AND d.decision_id=c.causation_id AND d.plan_revision=c.target_plan_revision)) OR (c.causation_kind='action_result' AND EXISTS (SELECT 1 FROM execass_logical_effects e WHERE e.delegation_id=c.delegation_id AND e.logical_effect_id=c.causation_id AND e.continuation_id!=c.continuation_id AND e.updated_at<=c.created_at)) OR (c.causation_kind IN ('recovery','resume','routine_occurrence') AND EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.aggregate_id=c.delegation_id AND o.aggregate_revision=c.target_delegation_revision AND o.causation_id=c.causation_id))))"),
            ("continuation_operation_history", "SELECT 'continuation_operation_history:'||h.event_id||':authority_or_accounting_reference' FROM execass_continuation_operation_history h WHERE h.delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.event_id=h.event_id AND o.aggregate_id=h.delegation_id AND o.event_name='execass.v1.continuation.claimed_or_result_recorded') OR NOT EXISTS (SELECT 1 FROM execass_receipts receipt WHERE receipt.delegation_id=h.delegation_id AND receipt.causation_event_id=h.event_id) OR NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.event_id=h.claim_event_id AND o.aggregate_id=h.delegation_id AND o.event_name='execass.v1.continuation.claimed_or_result_recorded') OR NOT EXISTS (SELECT 1 FROM execass_receipts receipt WHERE receipt.receipt_id=h.claim_receipt_id AND receipt.delegation_id=h.delegation_id AND receipt.causation_event_id=h.claim_event_id) OR NOT EXISTS (SELECT 1 FROM execass_continuations c WHERE c.continuation_id=h.continuation_id AND c.delegation_id=h.delegation_id AND c.action_id=h.action_id AND c.job_id=h.job_id) OR NOT EXISTS (SELECT 1 FROM execass_action_branches a WHERE a.action_id=h.action_id AND a.delegation_id=h.delegation_id) OR NOT EXISTS (SELECT 1 FROM execass_runtime_host_generations g WHERE g.generation=h.runtime_host_generation AND g.host_instance_id=h.runtime_host_instance_id) OR NOT EXISTS (SELECT 1 FROM execass_authority_provenance a WHERE a.authority_provenance_id=h.runtime_authority_provenance_id) OR (h.technical_quota_snapshot_id IS NULL AND json_array_length(h.technical_resource_reservation_set_json)!=0) OR (h.technical_quota_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM execass_technical_resource_quota_snapshots q WHERE q.quota_snapshot_id=h.technical_quota_snapshot_id AND q.delegation_id=h.delegation_id)) OR (h.operation='claim')!=(h.event_id=h.claim_event_id) OR (h.operation='claim' AND h.technical_resource_evidence_digest IS NOT NULL) OR (h.operation!='claim' AND (h.technical_resource_evidence_digest IS NULL OR length(h.technical_resource_evidence_digest)!=71 OR substr(h.technical_resource_evidence_digest,1,7)!='sha256:' OR substr(h.technical_resource_evidence_digest,8) GLOB '*[^0-9a-f]*')))"),
            ("effect", "SELECT 'effect:'||logical_effect_id||':continuation' FROM execass_logical_effects e WHERE delegation_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_continuations c WHERE c.delegation_id=e.delegation_id AND c.continuation_id=e.continuation_id)"),
            ("attempt", "SELECT 'attempt:'||a.attempt_id||':claim_ancestry_or_status' FROM execass_provider_attempts a WHERE a.delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_logical_effects e WHERE e.delegation_id=a.delegation_id AND e.logical_effect_id=a.logical_effect_id AND e.continuation_id=a.continuation_id) OR NOT EXISTS (SELECT 1 FROM execass_continuation_operation_history h WHERE h.operation='claim' AND h.event_id=a.claim_event_id AND h.claim_receipt_id=a.claim_receipt_id AND h.delegation_id=a.delegation_id AND h.continuation_id=a.continuation_id AND h.action_id=a.action_id AND h.continuation_fencing_token=a.fencing_token AND h.runtime_host_generation=a.host_generation AND h.runtime_host_instance_id=a.host_instance_id AND h.runtime_fencing_token=a.runtime_fencing_token) OR (a.status IN ('prepared','invoking') AND (a.provider_response_digest IS NOT NULL OR a.remote_effect_id IS NOT NULL OR a.finished_at IS NOT NULL)) OR (a.status IN ('succeeded','failed','outcome_unknown','reconciled_absent','reconciled_present') AND (a.provider_response_digest IS NULL OR a.finished_at IS NULL)))"),
            ("tombstone", "SELECT 'tombstone:'||t.tombstone_id||':effect' FROM execass_effect_tombstones t WHERE t.delegation_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_logical_effects e WHERE e.delegation_id=t.delegation_id AND e.logical_effect_id=t.logical_effect_id)"),
            ("technical_resource_quota_snapshot", "SELECT 'technical_resource_quota_snapshot:'||q.quota_snapshot_id||':delegation' FROM execass_technical_resource_quota_snapshots q WHERE q.delegation_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_delegations d WHERE d.delegation_id=q.delegation_id)"),
            ("technical_resource_requirement_set", "SELECT 'technical_resource_requirement_set:'||s.requirement_set_id||':snapshot_effect_action_or_manifest' FROM execass_technical_resource_requirement_sets s WHERE s.delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_technical_resource_quota_snapshots q WHERE q.quota_snapshot_id=s.quota_snapshot_id AND q.delegation_id=s.delegation_id) OR NOT EXISTS (SELECT 1 FROM execass_logical_effects e JOIN execass_continuations c ON c.continuation_id=e.continuation_id AND c.delegation_id=e.delegation_id WHERE e.logical_effect_id=s.logical_effect_id AND e.delegation_id=s.delegation_id AND e.manifest_digest=s.manifest_digest AND c.action_id=s.action_id) OR NOT EXISTS (SELECT 1 FROM execass_action_branches a WHERE a.action_id=s.action_id AND a.delegation_id=s.delegation_id))"),
            ("technical_resource_requirement", "SELECT 'technical_resource_requirement:'||requirement.requirement_set_id||':'||requirement.technical_resource_kind||':'||requirement.unit||':set_snapshot_or_quota_entry' FROM execass_technical_resource_requirements requirement JOIN execass_technical_resource_requirement_sets s ON s.requirement_set_id=requirement.requirement_set_id WHERE s.delegation_id=?1 AND (requirement.quota_snapshot_id!=s.quota_snapshot_id OR NOT EXISTS (SELECT 1 FROM execass_technical_resource_quota_entries q WHERE q.quota_snapshot_id=requirement.quota_snapshot_id AND q.technical_resource_kind=requirement.technical_resource_kind AND q.unit=requirement.unit))"),
            ("technical_resource_reservation", "SELECT 'technical_resource_reservation:'||r.reservation_id||':accounting_provenance' FROM execass_technical_resource_reservations r WHERE r.delegation_id=?1 AND (NOT EXISTS (SELECT 1 FROM execass_logical_effects e WHERE e.delegation_id=r.delegation_id AND e.logical_effect_id=r.logical_effect_id AND e.continuation_id=r.continuation_id) OR NOT EXISTS (SELECT 1 FROM execass_continuations c WHERE c.delegation_id=r.delegation_id AND c.continuation_id=r.continuation_id) OR NOT EXISTS (SELECT 1 FROM execass_technical_resource_quota_snapshots q WHERE q.quota_snapshot_id=r.quota_snapshot_id AND q.delegation_id=r.delegation_id) OR NOT EXISTS (SELECT 1 FROM execass_technical_resource_quota_entries q WHERE q.quota_snapshot_id=r.quota_snapshot_id AND q.technical_resource_kind=r.technical_resource_kind AND q.unit=r.unit) OR NOT EXISTS (SELECT 1 FROM execass_technical_resource_requirement_sets s JOIN execass_technical_resource_requirements requirement ON requirement.requirement_set_id=s.requirement_set_id AND requirement.quota_snapshot_id=s.quota_snapshot_id JOIN execass_logical_effects e ON e.logical_effect_id=s.logical_effect_id AND e.delegation_id=s.delegation_id JOIN execass_continuations c ON c.continuation_id=e.continuation_id AND c.delegation_id=e.delegation_id WHERE s.quota_snapshot_id=r.quota_snapshot_id AND s.delegation_id=r.delegation_id AND s.logical_effect_id=r.logical_effect_id AND s.action_id=c.action_id AND s.manifest_digest=e.manifest_digest AND requirement.technical_resource_kind=r.technical_resource_kind AND requirement.unit=r.unit AND requirement.amount_required=r.amount_reserved) OR NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.event_id=r.claim_event_id AND o.aggregate_id=r.delegation_id AND o.event_name='execass.v1.continuation.claimed_or_result_recorded') OR NOT EXISTS (SELECT 1 FROM execass_receipts receipt WHERE receipt.receipt_id=r.claim_receipt_id AND receipt.delegation_id=r.delegation_id AND receipt.causation_event_id=r.claim_event_id) OR NOT EXISTS (SELECT 1 FROM execass_continuation_operation_history h WHERE h.event_id=r.claim_event_id AND h.claim_event_id=r.claim_event_id AND h.claim_receipt_id=r.claim_receipt_id AND h.operation='claim' AND h.continuation_id=r.continuation_id AND h.delegation_id=r.delegation_id AND h.continuation_fencing_token=r.continuation_fencing_token AND h.runtime_host_generation=r.runtime_host_generation AND h.runtime_fencing_token=r.runtime_fencing_token AND h.technical_quota_snapshot_id=r.quota_snapshot_id))"),
            ("technical_resource_actual", "SELECT 'technical_resource_actual:'||a.technical_resource_actual_id||':reservation_provenance' FROM execass_technical_resource_actuals a WHERE a.delegation_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_technical_resource_reservations r WHERE r.delegation_id=a.delegation_id AND r.reservation_id=a.reservation_id AND r.status='settled' AND a.amount_actual<=r.amount_reserved AND a.continuation_fencing_token=r.continuation_fencing_token AND a.runtime_host_generation=r.runtime_host_generation AND a.runtime_fencing_token=r.runtime_fencing_token)"),
            ("technical_resource_terminal_operation", "SELECT 'technical_resource_reservation:'||r.reservation_id||':terminal_operation' FROM execass_technical_resource_reservations r WHERE r.delegation_id=?1 AND ((r.status='reconciliation_required' AND NOT EXISTS (SELECT 1 FROM execass_continuation_operation_history h WHERE h.claim_event_id=r.claim_event_id AND h.delegation_id=r.delegation_id AND h.operation='settle' AND h.continuation_fencing_token=r.continuation_fencing_token)) OR (r.status='settled' AND NOT EXISTS (SELECT 1 FROM execass_continuation_operation_history h WHERE h.claim_event_id=r.claim_event_id AND h.delegation_id=r.delegation_id AND h.operation IN ('settle','reconcile') AND h.continuation_fencing_token=r.continuation_fencing_token)) OR (r.status='released' AND NOT EXISTS (SELECT 1 FROM execass_continuation_operation_history h WHERE h.claim_event_id=r.claim_event_id AND h.delegation_id=r.delegation_id AND h.operation='reconcile' AND h.continuation_fencing_token=r.continuation_fencing_token)) OR (r.status='expired' AND NOT EXISTS (SELECT 1 FROM execass_continuation_operation_history h WHERE h.claim_event_id=r.claim_event_id AND h.delegation_id=r.delegation_id AND h.operation='expire' AND h.continuation_fencing_token=r.continuation_fencing_token)))"),
            ("receipt", "SELECT 'receipt:'||receipt_id||':parent_or_causation' FROM execass_receipts r WHERE delegation_id=?1 AND ((receipt_sequence=1 AND parent_receipt_id IS NOT NULL) OR (receipt_sequence>1 AND NOT EXISTS (SELECT 1 FROM execass_receipts p WHERE p.delegation_id=r.delegation_id AND p.receipt_id=r.parent_receipt_id AND p.receipt_sequence=r.receipt_sequence-1)) OR NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.event_id=r.causation_event_id AND o.aggregate_id=r.delegation_id AND o.causation_id=r.causation_id AND o.aggregate_revision=r.state_revision) OR r.state_revision>(SELECT state_revision FROM execass_delegations d WHERE d.delegation_id=r.delegation_id))"),
            ("outbox", "SELECT 'outbox:'||event_id||':aggregate' FROM execass_outbox_events WHERE aggregate_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_delegations d WHERE d.delegation_id=aggregate_id)"),
            ("link", "SELECT 'link:'||link_id||':outbox' FROM execass_authority_links l WHERE delegation_id=?1 AND NOT EXISTS (SELECT 1 FROM execass_outbox_events o WHERE o.event_id=l.outbox_event_id AND o.aggregate_id=l.delegation_id AND o.aggregate_revision=l.delegation_state_revision AND o.correlation_id=l.correlation_id AND o.causation_id=l.causation_id AND o.occurred_at=l.linked_at AND o.event_name='execass.v1.delegation.transitioned')"),
            ("link_revision", "SELECT 'link:'||link_id||':revision_gap' FROM execass_authority_links l WHERE delegation_id=?1 AND link_revision != (SELECT COUNT(*) FROM execass_authority_links p WHERE p.delegation_id=l.delegation_id AND (p.link_revision<l.link_revision OR (p.link_revision=l.link_revision AND p.link_id<=l.link_id)))"),
        ];
        let mut violations = Vec::new();
        for (_name, sql) in checks {
            violations.extend(string_rows(&conn, sql, delegation_id)?);
        }
        for link in resolve_link_identities(&conn, delegation_id)? {
            if resolve_source_id(&conn, link.0, &link.1).is_err() {
                violations.push(format!("link:{}:source", link.2));
            }
        }
        validate_operation_history_resources(&conn, delegation_id, &mut violations)?;
        violations.sort();
        violations.dedup();
        let report = DelegationReachabilityReport {
            delegation_id: delegation_id.to_string(),
            delegation_state_revision: delegation.state_revision,
            authority_provenance: record_refs(&conn, "SELECT authority_provenance_id, 0 FROM (SELECT authority_provenance_id FROM execass_delegations WHERE delegation_id=?1 UNION SELECT created_by_authority_provenance_id FROM execass_plans WHERE delegation_id=?1 UNION SELECT authority_provenance_id FROM execass_plan_amendments WHERE delegation_id=?1 UNION SELECT resolved_by_authority_provenance_id FROM execass_decisions WHERE delegation_id=?1 AND resolved_by_authority_provenance_id IS NOT NULL UNION SELECT accepted_by_authority_provenance_id FROM execass_accepted_confirmation_grants WHERE delegation_id=?1)", delegation_id)?,
            plans: record_refs(&conn, "SELECT plan_id, plan_revision FROM execass_plans WHERE delegation_id=?1", delegation_id)?,
            plan_amendments: record_refs(&conn, "SELECT amendment_id, amendment_revision FROM execass_plan_amendments WHERE delegation_id=?1", delegation_id)?,
            outcome_criteria: record_refs(&conn, "SELECT criterion_id, criteria_revision FROM execass_outcome_criteria WHERE delegation_id=?1", delegation_id)?,
            verifier_results: record_refs(&conn, "SELECT verifier_result_id, result_revision FROM execass_verifier_results WHERE delegation_id=?1", delegation_id)?,
            decisions: record_refs(&conn, "SELECT decision_id, decision_revision FROM execass_decisions WHERE delegation_id=?1", delegation_id)?,
            confirmation_challenges: record_refs(&conn, "SELECT challenge_id, decision_revision FROM execass_confirmation_challenges WHERE delegation_id=?1", delegation_id)?,
            accepted_confirmation_grants: record_refs(&conn, "SELECT grant_id, 0 FROM execass_accepted_confirmation_grants WHERE delegation_id=?1", delegation_id)?,
            continuations: record_refs(&conn, "SELECT continuation_id, target_delegation_revision FROM execass_continuations WHERE delegation_id=?1", delegation_id)?,
            continuation_operation_history: record_refs(&conn, "SELECT event_id, continuation_fencing_token FROM execass_continuation_operation_history WHERE delegation_id=?1", delegation_id)?,
            logical_effects: record_refs(&conn, "SELECT logical_effect_id, 0 FROM execass_logical_effects WHERE delegation_id=?1", delegation_id)?,
            provider_attempts: record_refs(&conn, "SELECT attempt_id, attempt_number FROM execass_provider_attempts WHERE delegation_id=?1", delegation_id)?,
            effect_tombstones: record_refs(&conn, "SELECT tombstone_id, 0 FROM execass_effect_tombstones WHERE delegation_id=?1", delegation_id)?,
            technical_resource_quota_snapshots: record_refs(&conn, "SELECT quota_snapshot_id, policy_revision FROM execass_technical_resource_quota_snapshots WHERE delegation_id=?1", delegation_id)?,
            technical_resource_quota_entries: record_refs(&conn, "SELECT q.quota_snapshot_id||':'||q.technical_resource_kind||':'||q.unit, 0 FROM execass_technical_resource_quota_entries q JOIN execass_technical_resource_quota_snapshots s ON s.quota_snapshot_id=q.quota_snapshot_id WHERE s.delegation_id=?1", delegation_id)?,
            technical_resource_requirement_sets: record_refs(&conn, "SELECT requirement_set_id, 0 FROM execass_technical_resource_requirement_sets WHERE delegation_id=?1", delegation_id)?,
            technical_resource_requirements: record_refs(&conn, "SELECT requirement.requirement_set_id||':'||requirement.technical_resource_kind||':'||requirement.unit, 0 FROM execass_technical_resource_requirements requirement JOIN execass_technical_resource_requirement_sets s ON s.requirement_set_id=requirement.requirement_set_id WHERE s.delegation_id=?1", delegation_id)?,
            technical_resource_reservations: record_refs(&conn, "SELECT reservation_id, 0 FROM execass_technical_resource_reservations WHERE delegation_id=?1", delegation_id)?,
            technical_resource_actuals: record_refs(&conn, "SELECT technical_resource_actual_id, 0 FROM execass_technical_resource_actuals WHERE delegation_id=?1", delegation_id)?,
            receipts: record_refs(&conn, "SELECT receipt_id, receipt_sequence FROM execass_receipts WHERE delegation_id=?1", delegation_id)?,
            outbox_events: record_refs(&conn, "SELECT event_id, global_sequence FROM execass_outbox_events WHERE aggregate_id=?1", delegation_id)?,
            authority_links: record_refs(&conn, "SELECT link_id, link_revision FROM execass_authority_links WHERE delegation_id=?1", delegation_id)?,
            violations,
        };
        Ok(if report.violations.is_empty() {
            DelegationReachabilityOutcome::Valid(report)
        } else {
            DelegationReachabilityOutcome::Invalid(report)
        })
    }
}

fn owner_kind_str(kind: AuthorityOwnerKind) -> &'static str {
    match kind {
        AuthorityOwnerKind::Agent => "agent",
        AuthorityOwnerKind::Session => "session",
        AuthorityOwnerKind::Run => "run",
        AuthorityOwnerKind::Job => "job",
        AuthorityOwnerKind::Project => "project",
        AuthorityOwnerKind::Board => "board",
        AuthorityOwnerKind::BoardCard => "board_card",
        AuthorityOwnerKind::Message => "message",
        AuthorityOwnerKind::MailThread => "mail_thread",
        AuthorityOwnerKind::MailMessage => "mail_message",
    }
}

fn canonical_parent_bindings(checks: &[AuthorityOwnershipCheck]) -> Vec<(String, String, String)> {
    let mut bindings = checks
        .iter()
        .map(|check| {
            (
                check.link_id.clone(),
                owner_kind_str(check.owner_kind).into(),
                check.expected_owner_id.clone(),
            )
        })
        .collect::<Vec<_>>();
    bindings.sort();
    bindings
}

fn persisted_parent_bindings(
    conn: &Connection,
    event_id: &str,
) -> Result<Vec<(String, String, String)>> {
    Ok(conn
        .prepare(
            "SELECT b.link_id,b.owner_kind,b.expected_owner_id FROM execass_authority_parent_bindings b JOIN execass_authority_links l ON l.link_id=b.link_id WHERE l.outbox_event_id=?1 ORDER BY b.link_id",
        )?
        .query_map(params![event_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn validate_ownership_checks(
    links: &[NewAuthorityLink],
    checks: &[AuthorityOwnershipCheck],
) -> Result<()> {
    for link in links {
        let required = match link.target {
            AuthorityLinkTarget::Session { .. } | AuthorityLinkTarget::Job { .. } => {
                Some(AuthorityOwnerKind::Agent)
            }
            AuthorityLinkTarget::Run { .. } => Some(AuthorityOwnerKind::Session),
            AuthorityLinkTarget::JobRun { .. } => Some(AuthorityOwnerKind::Job),
            AuthorityLinkTarget::Task { .. } => Some(AuthorityOwnerKind::Project),
            AuthorityLinkTarget::BoardCard { .. } => Some(AuthorityOwnerKind::Board),
            AuthorityLinkTarget::MailMessage { .. } => Some(AuthorityOwnerKind::MailThread),
            AuthorityLinkTarget::ArtifactAttachment { .. } => Some(AuthorityOwnerKind::Message),
            AuthorityLinkTarget::ArtifactBoardCardAsset { .. } => {
                Some(AuthorityOwnerKind::BoardCard)
            }
            AuthorityLinkTarget::ArtifactMailAttachment { .. } => {
                Some(AuthorityOwnerKind::MailMessage)
            }
            AuthorityLinkTarget::AssistantToolCallAudit { .. } => Some(AuthorityOwnerKind::Session),
            AuthorityLinkTarget::ToolCall { .. } => Some(AuthorityOwnerKind::Run),
            AuthorityLinkTarget::Board { .. }
            | AuthorityLinkTarget::MailThread { .. }
            | AuthorityLinkTarget::SecurityAuditEvent { .. }
            | AuthorityLinkTarget::Unsupported { .. } => None,
        };
        let matches = checks
            .iter()
            .filter(|check| check.link_id == link.link_id)
            .collect::<Vec<_>>();
        match (required, matches.as_slice()) {
            (Some(owner_kind), [check]) if check.owner_kind == owner_kind => {
                require_text("expected_owner_id", &check.expected_owner_id)?;
            }
            (None, []) => {}
            _ => bail!(
                "authority reference {} requires exactly {:?} ownership",
                link.link_id,
                required
            ),
        }
    }
    if checks.iter().any(|check| {
        links
            .iter()
            .filter(|link| link.link_id == check.link_id)
            .count()
            != 1
    }) {
        bail!("ownership check does not identify exactly one authority reference");
    }
    Ok(())
}

fn validate_operation_history_resources(
    conn: &Connection,
    delegation_id: &str,
    violations: &mut Vec<String>,
) -> Result<()> {
    let mut statement = conn.prepare(
        r#"SELECT event_id,claim_event_id,operation,
                  technical_resource_reservation_set_json,
                  technical_resource_reservation_set_digest,
                  technical_resource_evidence_digest
           FROM execass_continuation_operation_history
           WHERE delegation_id=?1
           ORDER BY event_id"#,
    )?;
    let histories = statement
        .query_map(params![delegation_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (event_id, claim_event_id, operation, canonical_json, stored_digest, evidence_digest) in
        histories
    {
        let expected_identities =
            reservation_identities_for_claim(conn, delegation_id, &claim_event_id)?;
        let retained_identities =
            serde_json::from_str::<Vec<TechnicalResourceReservationIdentity>>(&canonical_json);
        let resource_set_valid = retained_identities
            .as_ref()
            .ok()
            .filter(|identities| *identities == &expected_identities)
            .and_then(|identities| serde_json::to_string(identities).ok())
            .is_some_and(|rebuilt_json| rebuilt_json == canonical_json)
            && super::claim::resource_identity_set_digest(&expected_identities)? == stored_digest;
        if !resource_set_valid {
            violations.push(format!(
                "continuation_operation_history:{event_id}:resource_set"
            ));
        }

        if operation == "settle"
            && evidence_digest.as_deref()
                != Some(
                    technical_resource_actual_set_digest_for_claim(
                        conn,
                        delegation_id,
                        &claim_event_id,
                    )?
                    .as_str(),
                )
        {
            violations.push(format!(
                "continuation_operation_history:{event_id}:evidence_digest"
            ));
        }
        if matches!(operation.as_str(), "expire" | "recover" | "reconcile") {
            let expected = technical_resource_lifecycle_evidence_digest_for_event(
                conn,
                &event_id,
                delegation_id,
                &claim_event_id,
            )?;
            if evidence_digest.as_deref() != expected.as_deref() {
                violations.push(format!(
                    "continuation_operation_history:{event_id}:authoritative_evidence_reference"
                ));
            }
        }
    }
    Ok(())
}

fn technical_resource_lifecycle_evidence_digest_for_event(
    conn: &Connection,
    event_id: &str,
    delegation_id: &str,
    claim_event_id: &str,
) -> Result<Option<String>> {
    let evidence = conn
        .prepare(
            r#"SELECT e.authority_link_id,e.authority_kind,e.source_id,e.authoritative_revision
               FROM execass_receipts receipt
               JOIN execass_receipt_evidence_refs e ON e.receipt_id=receipt.receipt_id
               WHERE receipt.delegation_id=?1 AND receipt.causation_event_id=?2
               ORDER BY e.ordinal"#,
        )?
        .query_map(params![delegation_id, event_id], |row| {
            Ok(ReceiptEvidenceInput {
                authority_link_id: row.get(0)?,
                kind: row.get(1)?,
                source_id: row.get(2)?,
                authoritative_revision: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let reference_digest =
        match super::claim::technical_resource_lifecycle_evidence_reference_digest(&evidence) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
    let actual_set_digest =
        technical_resource_actual_set_digest_for_claim(conn, delegation_id, claim_event_id)?;
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.lifecycle-evidence.v1\0");
    digest.update(reference_digest.as_bytes());
    digest.update(b"\0");
    digest.update(actual_set_digest.as_bytes());
    Ok(Some(format!("sha256:{:x}", digest.finalize())))
}

fn reservation_identities_for_claim(
    conn: &Connection,
    delegation_id: &str,
    claim_event_id: &str,
) -> Result<Vec<TechnicalResourceReservationIdentity>> {
    Ok(conn
        .prepare(
            r#"SELECT reservation_id,quota_snapshot_id,logical_effect_id,
                      technical_resource_kind,unit,amount_reserved
               FROM execass_technical_resource_reservations
               WHERE delegation_id=?1 AND claim_event_id=?2
               ORDER BY technical_resource_kind,unit"#,
        )?
        .query_map(params![delegation_id, claim_event_id], |row| {
            Ok(TechnicalResourceReservationIdentity {
                reservation_id: row.get(0)?,
                quota_snapshot_id: row.get(1)?,
                logical_effect_id: row.get(2)?,
                technical_resource_kind: row.get(3)?,
                unit: row.get(4)?,
                amount_reserved: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn technical_resource_actual_set_digest_for_claim(
    conn: &Connection,
    delegation_id: &str,
    claim_event_id: &str,
) -> Result<String> {
    let mut canonical = conn
        .prepare(
            r#"SELECT a.reservation_id,a.amount_actual,a.evidence_digest
               FROM execass_technical_resource_actuals a
               JOIN execass_technical_resource_reservations r
                 ON r.delegation_id=a.delegation_id AND r.reservation_id=a.reservation_id
               WHERE r.delegation_id=?1 AND r.claim_event_id=?2"#,
        )?
        .query_map(params![delegation_id, claim_event_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    canonical.sort();
    let canonical_json = serde_json::to_string(&canonical)?;
    let mut digest = Sha256::new();
    digest.update(b"carsinos.execass.technical-resource.actual-set.v1\0");
    digest.update(canonical_json.as_bytes());
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn string_rows(conn: &Connection, sql: &str, delegation_id: &str) -> Result<Vec<String>> {
    Ok(conn
        .prepare(sql)?
        .query_map(params![delegation_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}

fn record_refs(
    conn: &Connection,
    sql: &str,
    delegation_id: &str,
) -> Result<Vec<ReachabilityRecordRef>> {
    let mut records = conn
        .prepare(sql)?
        .query_map(params![delegation_id], |row| {
            Ok(ReachabilityRecordRef {
                record_id: row.get(0)?,
                revision: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    records.sort_by(|left, right| {
        left.record_id
            .cmp(&right.record_id)
            .then(left.revision.cmp(&right.revision))
    });
    records.dedup();
    Ok(records)
}

fn resolve_link_identities(
    conn: &Connection,
    delegation_id: &str,
) -> Result<Vec<(AuthorityLinkKind, String, String)>> {
    let mut statement = conn.prepare("SELECT link_id, authority_kind, session_id, run_id, job_id, job_run_id, task_id, board_id, board_card_id, mail_thread_id, mail_message_id, attachment_id, board_card_asset_id, mail_attachment_id, security_audit_event_id, assistant_tool_call_audit_event_id, tool_call_id FROM execass_authority_links WHERE delegation_id=?1 ORDER BY link_id")?;
    let rows = statement
        .query_map(params![delegation_id], |row| {
            let kind: AuthorityLinkKind = row.get(1)?;
            let source_id = (2..17)
                .find_map(|index| row.get::<_, Option<String>>(index).ok().flatten())
                .ok_or(rusqlite::Error::InvalidQuery)?;
            Ok((kind, source_id, row.get(0)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

fn outbox_by_duplicate_identity(
    conn: &Connection,
    duplicate_identity: &str,
) -> Result<Option<OutboxEventRecord>> {
    let id = conn
        .query_row(
            "SELECT event_id FROM execass_outbox_events WHERE duplicate_identity = ?1",
            params![duplicate_identity],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    id.map(|id| get_outbox(conn, &id))
        .transpose()
        .map(Option::flatten)
}

fn canonical_requested_members(
    links: &[NewAuthorityLink],
) -> Result<Vec<(String, AuthorityLinkKind, String, i64)>> {
    let mut members = links
        .iter()
        .map(|link| {
            Ok((
                link.link_id.clone(),
                link.target.kind().context("unsupported authority target")?,
                link.target
                    .source_id()
                    .context("unsupported authority target")?
                    .to_string(),
                0,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    members.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.as_str().cmp(right.1.as_str()))
            .then(left.2.cmp(&right.2))
            .then(left.3.cmp(&right.3))
    });
    Ok(members)
}

fn canonical_members(
    links: &[AuthorityLinkProjection],
) -> Vec<(String, AuthorityLinkKind, String, i64)> {
    let mut members = links
        .iter()
        .map(|link| {
            (
                link.link_id.clone(),
                link.kind,
                link.source_id.clone(),
                link.authoritative_revision,
            )
        })
        .collect::<Vec<_>>();
    members.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.as_str().cmp(right.1.as_str()))
            .then(left.2.cmp(&right.2))
            .then(left.3.cmp(&right.3))
    });
    members
}

fn insert_link(
    conn: &Connection,
    command: &AppendAuthorityLineageCommand,
    link: &NewAuthorityLink,
) -> Result<()> {
    let kind = link.target.kind().context("unsupported authority target")?;
    let source_id = link
        .target
        .source_id()
        .context("unsupported authority target")?;
    let next_revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(link_revision), 0) + 1 FROM execass_authority_links WHERE delegation_id = ?1",
        params![command.delegation_id], |row| row.get(0),
    )?;
    let mut values: [Option<&str>; 15] = [None; 15];
    values[target_slot(kind)] = Some(source_id);
    conn.execute(
        r#"INSERT INTO execass_authority_links (
          link_id, delegation_id, link_revision, delegation_state_revision,
          correlation_id, causation_id, outbox_event_id, authority_kind,
          session_id, run_id, job_id, job_run_id, task_id, board_id, board_card_id,
          mail_thread_id, mail_message_id, attachment_id, board_card_asset_id,
          mail_attachment_id, security_audit_event_id, assistant_tool_call_audit_event_id,
          tool_call_id, authoritative_revision, linked_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                  ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, 0, ?24)"#,
        params![
            link.link_id,
            command.delegation_id,
            next_revision,
            command.resulting_state_revision,
            command.write.correlation_id,
            command.write.causation_id,
            command.outbox_event.event_id,
            kind.as_str(),
            values[0],
            values[1],
            values[2],
            values[3],
            values[4],
            values[5],
            values[6],
            values[7],
            values[8],
            values[9],
            values[10],
            values[11],
            values[12],
            values[13],
            values[14],
            command.linked_at,
        ],
    )
    .context("failed inserting immutable authority lineage link")?;
    Ok(())
}

fn target_slot(kind: AuthorityLinkKind) -> usize {
    match kind {
        AuthorityLinkKind::Session => 0,
        AuthorityLinkKind::Run => 1,
        AuthorityLinkKind::Job => 2,
        AuthorityLinkKind::JobRun => 3,
        AuthorityLinkKind::Task => 4,
        AuthorityLinkKind::Board => 5,
        AuthorityLinkKind::BoardCard => 6,
        AuthorityLinkKind::MailThread => 7,
        AuthorityLinkKind::MailMessage => 8,
        AuthorityLinkKind::ArtifactAttachment => 9,
        AuthorityLinkKind::ArtifactBoardCardAsset => 10,
        AuthorityLinkKind::ArtifactMailAttachment => 11,
        AuthorityLinkKind::SecurityAuditEvent => 12,
        AuthorityLinkKind::AssistantToolCallAudit => 13,
        AuthorityLinkKind::ToolCall => 14,
    }
}

fn resolve_links_for_event(
    conn: &Connection,
    event_id: &str,
) -> Result<Vec<AuthorityLinkProjection>> {
    resolve_links(conn, "outbox_event_id = ?1", event_id)
}
fn resolve_links_for_delegation(
    conn: &Connection,
    delegation_id: &str,
) -> Result<Vec<AuthorityLinkProjection>> {
    resolve_links(conn, "delegation_id = ?1", delegation_id)
}
fn resolve_links(
    conn: &Connection,
    predicate: &str,
    value: &str,
) -> Result<Vec<AuthorityLinkProjection>> {
    let sql = format!(
        r#"SELECT link_id, delegation_id, link_revision, delegation_state_revision, authority_kind,
      session_id, run_id, job_id, job_run_id, task_id, board_id, board_card_id, mail_thread_id, mail_message_id,
      attachment_id, board_card_asset_id, mail_attachment_id, security_audit_event_id, assistant_tool_call_audit_event_id,
      tool_call_id, authoritative_revision, linked_at, outbox_event_id FROM execass_authority_links WHERE {predicate} ORDER BY link_revision, link_id"#
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement
        .query_map(params![value], |row| {
            let kind: AuthorityLinkKind = row.get(4)?;
            let source_id: String = (5..20)
                .find_map(|index| row.get::<_, Option<String>>(index).ok().flatten())
                .ok_or(rusqlite::Error::InvalidQuery)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                kind,
                source_id,
                row.get::<_, i64>(20)?,
                row.get::<_, i64>(21)?,
                row.get::<_, String>(22)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(
            |(
                link_id,
                delegation_id,
                link_revision,
                delegation_state_revision,
                kind,
                source_id,
                authoritative_revision,
                linked_at,
                outbox_event_id,
            )| {
                let location = resolve_source_id(conn, kind, &source_id)?;
                Ok(AuthorityLinkProjection {
                    link_id,
                    delegation_id,
                    link_revision,
                    delegation_state_revision,
                    kind,
                    source_id,
                    authoritative_revision,
                    linked_at,
                    outbox_event_id,
                    location,
                    reachable: true,
                })
            },
        )
        .collect()
}

fn resolve_target(
    conn: &Connection,
    target: &AuthorityLinkTarget,
) -> Result<AuthoritySourceLocation> {
    match target {
        AuthorityLinkTarget::Unsupported { kind } => {
            Err(AuthorityLineageError::Unsupported(*kind).into())
        }
        _ => match (target.kind(), target.source_id()) {
            (Some(kind), Some(source_id)) => resolve_source_id(conn, kind, source_id),
            _ => bail!("authority target has no supported identity"),
        },
    }
}

fn resolve_source_id(
    conn: &Connection,
    kind: AuthorityLinkKind,
    source_id: &str,
) -> Result<AuthoritySourceLocation> {
    let (table, column) = match kind {
        AuthorityLinkKind::Session => ("sessions", "session_id"),
        AuthorityLinkKind::Run => ("runs", "run_id"),
        AuthorityLinkKind::Job => ("jobs", "job_id"),
        AuthorityLinkKind::JobRun => ("job_runs", "job_run_id"),
        AuthorityLinkKind::Task => ("tasks", "task_id"),
        AuthorityLinkKind::Board => ("boards", "board_id"),
        AuthorityLinkKind::BoardCard => ("board_cards", "card_id"),
        AuthorityLinkKind::MailThread => ("agent_mail_threads", "thread_id"),
        AuthorityLinkKind::MailMessage => ("agent_mail_messages", "message_id"),
        AuthorityLinkKind::ArtifactAttachment => ("attachments", "attachment_id"),
        AuthorityLinkKind::ArtifactBoardCardAsset => ("board_card_assets", "card_asset_id"),
        AuthorityLinkKind::ArtifactMailAttachment => ("agent_mail_attachments", "attachment_id"),
        AuthorityLinkKind::AssistantToolCallAudit => ("assistant_tool_calls_audit", "event_id"),
        AuthorityLinkKind::ToolCall => ("tool_calls", "tool_call_id"),
        AuthorityLinkKind::SecurityAuditEvent => {
            if exists(conn, "security_audit_events", "event_id", source_id)? {
                return Ok(AuthoritySourceLocation::Live);
            }
            if exists(conn, "security_audit_events_archive", "event_id", source_id)? {
                return Ok(AuthoritySourceLocation::Archived);
            }
            return Err(AuthorityLineageError::MissingSource {
                kind,
                source_id: source_id.to_string(),
            }
            .into());
        }
    };
    if exists(conn, table, column, source_id)? {
        Ok(AuthoritySourceLocation::Live)
    } else {
        Err(AuthorityLineageError::MissingSource {
            kind,
            source_id: source_id.to_string(),
        }
        .into())
    }
}

fn exists(conn: &Connection, table: &str, column: &str, source_id: &str) -> Result<bool> {
    let sql = format!("SELECT 1 FROM {table} WHERE {column} = ?1 LIMIT 1");
    Ok(conn
        .query_row(&sql, params![source_id], |_| Ok(()))
        .optional()?
        .is_some())
}

fn ownership_mismatch(
    conn: &Connection,
    link: &NewAuthorityLink,
    check: &AuthorityOwnershipCheck,
) -> Result<Option<(String, Option<String>)>> {
    let source_id = link
        .target
        .source_id()
        .context("unsupported authority target")?;
    let sql = match (&link.target, check.owner_kind) {
        (AuthorityLinkTarget::Session { .. }, AuthorityOwnerKind::Agent) => {
            "SELECT agent_id FROM sessions WHERE session_id=?1"
        }
        (AuthorityLinkTarget::Run { .. }, AuthorityOwnerKind::Session) => {
            "SELECT session_id FROM runs WHERE run_id=?1"
        }
        (AuthorityLinkTarget::Job { .. }, AuthorityOwnerKind::Agent) => {
            "SELECT agent_id FROM jobs WHERE job_id=?1"
        }
        (AuthorityLinkTarget::JobRun { .. }, AuthorityOwnerKind::Job) => {
            "SELECT job_id FROM job_runs WHERE job_run_id=?1"
        }
        (AuthorityLinkTarget::Task { .. }, AuthorityOwnerKind::Project) => {
            "SELECT project_id FROM tasks WHERE task_id=?1"
        }
        (AuthorityLinkTarget::BoardCard { .. }, AuthorityOwnerKind::Board) => {
            "SELECT board_id FROM board_cards WHERE card_id=?1"
        }
        (AuthorityLinkTarget::MailMessage { .. }, AuthorityOwnerKind::MailThread) => {
            "SELECT thread_id FROM agent_mail_messages WHERE message_id=?1"
        }
        (AuthorityLinkTarget::ArtifactAttachment { .. }, AuthorityOwnerKind::Message) => {
            "SELECT message_id FROM attachments WHERE attachment_id=?1"
        }
        (AuthorityLinkTarget::ArtifactBoardCardAsset { .. }, AuthorityOwnerKind::BoardCard) => {
            "SELECT card_id FROM board_card_assets WHERE card_asset_id=?1"
        }
        (AuthorityLinkTarget::ArtifactMailAttachment { .. }, AuthorityOwnerKind::MailMessage) => {
            "SELECT message_id FROM agent_mail_attachments WHERE attachment_id=?1"
        }
        (AuthorityLinkTarget::ToolCall { .. }, AuthorityOwnerKind::Run) => {
            "SELECT run_id FROM tool_calls WHERE tool_call_id=?1"
        }
        (AuthorityLinkTarget::AssistantToolCallAudit { .. }, AuthorityOwnerKind::Session) => {
            "SELECT root_session_id FROM assistant_tool_calls_audit WHERE event_id=?1"
        }
        _ => bail!(
            "invalid ownership check for authority reference {}",
            link.link_id
        ),
    };
    let actual = conn
        .query_row(sql, params![source_id], |row| row.get::<_, String>(0))
        .optional()?;
    Ok(
        (actual.as_deref() != Some(check.expected_owner_id.as_str()))
            .then_some((check.expected_owner_id.clone(), actual)),
    )
}
