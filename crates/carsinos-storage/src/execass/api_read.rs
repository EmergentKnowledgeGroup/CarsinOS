//! Read-only canonical ExecAss facades for the versioned HTTP adapter.
//!
//! These methods intentionally return storage records rather than inventing a
//! second projection.  The gateway owns wire DTO conversion and authentication.

use super::projection::canonical_delivered_items;
use super::receipt_integrity::ReceiptIntegrityStore;
use super::redaction::ReceiptRedactor;
use super::rows::{get_delegation, get_plan_by_revision, list_criteria};
use super::store::ExecAssStore;
use super::types::*;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use rusqlite::{OptionalExtension, TransactionBehavior};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;
const MAX_LIST_LIMIT: u16 = 100;
const CURSOR_VERSION: &str = "execass.api.list.v1";

impl ExecAssStore {
    pub fn list_api_delegations(
        &self,
        query: &ApiDelegationListQuery,
        key: &ApiCursorKey,
    ) -> Result<ApiDelegationListPage> {
        if query.limit == 0 || query.limit > MAX_LIST_LIMIT {
            bail!("delegation list limit is out of range")
        }
        let after = query
            .cursor
            .as_deref()
            .map(|value| decode_cursor(value, key))
            .transpose()?;
        let conn = self.connection()?;
        conn.pragma_update(None, "query_only", "ON")?;
        let mut sql = String::from("SELECT delegation_id,state_revision,phase,run_control,updated_at FROM execass_delegations WHERE delegation_id!='execass-global-control-carrier'");
        let mut values: Vec<rusqlite::types::Value> = Vec::new();
        if let Some(phase) = query.phase {
            sql.push_str(" AND phase=?");
            values.push(phase.as_str().to_owned().into());
        }
        if let Some(run) = query.run_control {
            sql.push_str(" AND run_control=?");
            values.push(run.as_str().to_owned().into());
        }
        if let Some(cursor) = &after {
            if cursor.phase != query.phase.map(|v| v.as_str().to_owned())
                || cursor.run_control != query.run_control.map(|v| v.as_str().to_owned())
            {
                bail!("delegation cursor does not match filters")
            }
            sql.push_str(" AND (updated_at<? OR (updated_at=? AND delegation_id<?))");
            values.push(cursor.updated_at.into());
            values.push(cursor.updated_at.into());
            values.push(cursor.delegation_id.clone().into());
        }
        sql.push_str(" ORDER BY updated_at DESC,delegation_id DESC LIMIT ?");
        values.push((i64::from(query.limit) + 1).into());
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(rusqlite::params_from_iter(values))?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push(ApiDelegationListEntry {
                delegation_id: row.get(0)?,
                state_revision: row.get(1)?,
                phase: row.get(2)?,
                run_control: row.get(3)?,
                updated_at: row.get(4)?,
            });
        }
        let has_more = entries.len() > query.limit as usize;
        if has_more {
            entries.pop();
        }
        let next_cursor = if has_more {
            entries.last().map(|entry| {
                encode_cursor(
                    &ListCursor {
                        phase: query.phase.map(|v| v.as_str().to_owned()),
                        run_control: query.run_control.map(|v| v.as_str().to_owned()),
                        updated_at: entry.updated_at,
                        delegation_id: entry.delegation_id.clone(),
                    },
                    key,
                )
            })
        } else {
            None
        };
        Ok(ApiDelegationListPage {
            entries,
            next_cursor,
        })
    }

    pub fn read_api_delegation_detail(
        &self,
        delegation_id: &str,
    ) -> Result<Option<ApiDelegationDetail>> {
        if delegation_id == "execass-global-control-carrier" {
            return Ok(None);
        }
        let mut conn = self.connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let Some(delegation) = get_delegation(&tx, delegation_id)? else {
            return Ok(None);
        };
        let current_plan = delegation
            .current_plan_revision
            .map(|revision| get_plan_by_revision(&tx, delegation_id, revision))
            .transpose()?
            .flatten();
        if delegation.current_plan_revision.is_some() && current_plan.is_none() {
            bail!("delegation current plan is missing")
        }
        let criteria = delegation
            .current_criteria_revision
            .map(|revision| list_criteria(&tx, delegation_id, revision))
            .transpose()?
            .unwrap_or_default();
        let actions = list_actions(&tx, delegation_id)?;
        let continuations = list_continuations(&tx, delegation_id)?;
        let effects = list_effects(&tx, delegation_id)?;
        let recovery = list_recovery(&tx, delegation_id)?;
        let verifiers = list_verifiers(&tx, delegation_id)?;
        Ok(Some(ApiDelegationDetail {
            receipt_chain_head: delegation.receipt_chain_head_digest.clone(),
            delegation,
            current_plan,
            criteria,
            actions,
            continuations,
            effects,
            recovery,
            verifiers,
        }))
    }

    pub fn read_api_delegation_receipts(
        &self,
        delegation_id: &str,
    ) -> Result<Option<ApiDelegationReceiptPage>> {
        if delegation_id == "execass-global-control-carrier" {
            return Ok(None);
        }
        let mut conn = self.connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let Some(delegation) = get_delegation(&tx, delegation_id)? else {
            return Ok(None);
        };
        let mut stmt = tx.prepare("SELECT receipt_id,receipt_sequence,global_sequence,receipt_digest,previous_receipt_digest,global_previous_receipt_digest,key_id,key_generation,keyed_integrity_tag,previous_key_integrity_tag,redacted_summary,receipt_kind,subject_kind,subject_id,subject_revision,occurred_at,committed_at FROM execass_receipts WHERE delegation_id=?1 ORDER BY receipt_sequence,receipt_id")?;
        let mut receipts = stmt
            .query_map([delegation_id], |row| {
                Ok(ApiReceiptRead {
                    receipt_id: row.get(0)?,
                    delegation_sequence: row.get(1)?,
                    global_sequence: row.get(2)?,
                    receipt_digest: row.get(3)?,
                    previous_receipt_digest: row.get(4)?,
                    global_previous_receipt_digest: row.get(5)?,
                    key_id: row.get(6)?,
                    key_generation: row.get(7)?,
                    integrity_tag: row.get(8)?,
                    previous_key_integrity_tag: row.get(9)?,
                    safe_summary: row.get(10)?,
                    receipt_kind: row.get(11)?,
                    subject_kind: row.get(12)?,
                    subject_id: row.get(13)?,
                    subject_revision: row.get(14)?,
                    occurred_at: row.get(15)?,
                    committed_at: row.get(16)?,
                    evidence: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for receipt in &mut receipts {
            receipt.evidence = list_receipt_evidence(&tx, &receipt.receipt_id)?;
        }
        if receipts.len() as i64 != delegation.receipt_chain_count {
            bail!("delegation receipt count is corrupt")
        }
        if receipts
            .last()
            .map(|receipt| receipt.receipt_digest.as_str())
            != delegation.receipt_chain_head_digest.as_deref()
        {
            bail!("delegation receipt head is corrupt")
        }
        Ok(Some(ApiDelegationReceiptPage {
            delegation_id: delegation_id.to_owned(),
            chain_head: delegation.receipt_chain_head_digest,
            receipts,
        }))
    }

    pub fn read_api_current_decision(&self, decision_id: &str) -> Result<Option<ApiDecisionRead>> {
        let mut conn = self.connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Deferred)?;
        Ok(tx
            .query_row(
                r#"SELECT d.decision_id,d.delegation_id,d.decision_revision,d.delegation_revision,
                      d.plan_revision,d.policy_revision,d.decision_kind,d.status,d.result,
                      d.confirmed_logical_action_identity,d.manifest_digest,d.idempotency_key,
                      d.requested_at,d.resolved_at,d.resolved_by_authority_provenance_id,
                      d.exact_presented_action_json,d.recommendation,d.consequence,
                      d.alternatives_json,c.challenge_id,c.nonce_digest,c.expires_at,
                      g.grant_id,g.canonical_action_envelope_or_selector_json,
                      g.payload_and_material_operands_digest,g.connector_tool_identity,
                      g.connector_tool_version,g.declared_consequence,
                      authority.authenticated_ingress,authority.evidence_digest
               FROM execass_decisions d
               LEFT JOIN execass_confirmation_challenges c ON c.decision_id=d.decision_id
               LEFT JOIN execass_accepted_confirmation_grants g
                 ON g.decision_id=d.decision_id AND g.invalidated_at IS NULL
               LEFT JOIN execass_authority_provenance authority
                 ON authority.authority_provenance_id=d.resolved_by_authority_provenance_id
               WHERE d.decision_id=?1"#,
                [decision_id],
                |row| {
                    let connector_identity: Option<String> = row.get(25)?;
                    let connector_version: Option<String> = row.get(26)?;
                    Ok(ApiDecisionRead {
                        decision: DecisionRecord {
                            decision_id: row.get(0)?,
                            delegation_id: row.get(1)?,
                            decision_revision: row.get(2)?,
                            delegation_revision: row.get(3)?,
                            plan_revision: row.get(4)?,
                            policy_revision: row.get(5)?,
                            decision_kind: row.get(6)?,
                            status: row.get(7)?,
                            result: row.get(8)?,
                            confirmed_logical_action_identity: row.get(9)?,
                            manifest_digest: row.get(10)?,
                            idempotency_key: row.get(11)?,
                            requested_at: row.get(12)?,
                            resolved_at: row.get(13)?,
                            resolved_by_authority_provenance_id: row.get(14)?,
                        },
                        exact_presented_action_json: row.get(15)?,
                        recommendation: row.get(16)?,
                        consequence: row.get(17)?,
                        alternatives_json: row.get(18)?,
                        challenge_id: row.get(19)?,
                        challenge_nonce_digest: row.get(20)?,
                        challenge_expires_at: row.get(21)?,
                        accepted_grant: row.get::<_, Option<String>>(22)?.map(|grant_id| {
                            ApiAcceptedGrantRead {
                                grant_id,
                                canonical_action_envelope_or_selector_json: row
                                    .get(23)
                                    .expect("grant envelope"),
                                payload_and_material_operands_digest: row
                                    .get(24)
                                    .expect("grant digest"),
                                connector_tool_identity_and_version: match (
                                    connector_identity,
                                    connector_version,
                                ) {
                                    (Some(identity), Some(version)) => {
                                        Some(format!("{identity}@{version}"))
                                    }
                                    (Some(identity), None) => Some(identity),
                                    _ => None,
                                },
                                declared_consequence: row.get(27).expect("grant consequence"),
                            }
                        }),
                        resolved_owner: row.get::<_, Option<String>>(28)?.map(
                            |authenticated_ingress| ApiResolvedOwnerRead {
                                authenticated_ingress,
                                verified_evidence_ref: row.get(29).expect("authority evidence"),
                            },
                        ),
                    })
                },
            )
            .optional()?)
    }

    pub fn read_api_summary_with_delivery_metadata(
        &self,
        integrity: &ReceiptIntegrityStore,
        redactor: &ReceiptRedactor,
        query: &ExecAssProjectionQuery,
        metadata: &SummaryDeliveryMetadata,
    ) -> Result<(ExecAssExecutiveProjection, SummaryDeliveryOutcome)> {
        let projection = self.read_authoritative_projection(integrity, redactor, query)?;
        let command = SummaryDeliveryCommand {
            delivery_id: metadata.delivery_id.clone(),
            request_identity: metadata.request_identity.clone(),
            delivered_at: metadata.delivered_at,
            projection_version: projection.projection_version.clone(),
            through_global_sequence: projection.boundary.through_global_sequence,
            item_set_digest: projection.boundary.item_set_digest.clone(),
            items: canonical_delivered_items(&projection)?,
        };
        let outcome = self.record_summary_delivery(&projection, &command)?;
        Ok((projection, outcome))
    }
}

#[derive(Debug)]
struct ListCursor {
    phase: Option<String>,
    run_control: Option<String>,
    updated_at: i64,
    delegation_id: String,
}
fn encode_cursor(cursor: &ListCursor, key: &ApiCursorKey) -> String {
    let payload = format!(
        "{CURSOR_VERSION}\n{}\n{}\n{}\n{}",
        cursor.phase.as_deref().unwrap_or(""),
        cursor.run_control.as_deref().unwrap_or(""),
        cursor.updated_at,
        cursor.delegation_id
    );
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("fixed key");
    mac.update(payload.as_bytes());
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(payload),
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    )
}
fn decode_cursor(cursor: &str, key: &ApiCursorKey) -> Result<ListCursor> {
    let (payload, tag) = cursor
        .split_once('.')
        .context("delegation cursor is malformed")?;
    let payload = URL_SAFE_NO_PAD
        .decode(payload)
        .context("delegation cursor is malformed")?;
    let tag = URL_SAFE_NO_PAD
        .decode(tag)
        .context("delegation cursor is malformed")?;
    let mut mac = HmacSha256::new_from_slice(&key.0).expect("fixed key");
    mac.update(&payload);
    mac.verify_slice(&tag)
        .map_err(|_| anyhow::anyhow!("delegation cursor integrity check failed"))?;
    let text = String::from_utf8(payload).context("delegation cursor is malformed")?;
    let mut fields = text.split('\n');
    if fields.next() != Some(CURSOR_VERSION) {
        bail!("delegation cursor version is unsupported")
    };
    let phase = fields.next().filter(|v| !v.is_empty()).map(str::to_owned);
    let run_control = fields.next().filter(|v| !v.is_empty()).map(str::to_owned);
    let updated_at = fields
        .next()
        .context("delegation cursor is malformed")?
        .parse()
        .context("delegation cursor is malformed")?;
    let delegation_id = fields
        .next()
        .filter(|v| !v.is_empty())
        .context("delegation cursor is malformed")?
        .to_owned();
    if fields.next().is_some() {
        bail!("delegation cursor is malformed")
    };
    Ok(ListCursor {
        phase,
        run_control,
        updated_at,
        delegation_id,
    })
}

fn list_actions(conn: &rusqlite::Connection, id: &str) -> Result<Vec<ApiActionRead>> {
    let mut stmt=conn.prepare("SELECT action_id,action_revision,status,action_summary FROM execass_action_branches WHERE delegation_id=?1 ORDER BY action_revision,action_id")?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(ApiActionRead {
                action_id: row.get(0)?,
                action_revision: row.get(1)?,
                status: row.get(2)?,
                safe_summary: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
fn list_continuations(conn: &rusqlite::Connection, id: &str) -> Result<Vec<ContinuationRecord>> {
    let mut stmt=conn.prepare("SELECT continuation_id,delegation_id,target_delegation_revision,target_plan_revision,action_id,branch_kind,causation_kind,causation_id,status,job_id,lease_owner,lease_expires_at,fencing_token,host_generation,stop_epoch,global_stop_epoch,created_at,updated_at,completed_at FROM execass_continuations WHERE delegation_id=?1 ORDER BY created_at,continuation_id")?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(ContinuationRecord {
                continuation_id: row.get(0)?,
                delegation_id: row.get(1)?,
                target_delegation_revision: row.get(2)?,
                target_plan_revision: row.get(3)?,
                action_id: row.get(4)?,
                branch_kind: row.get(5)?,
                causation_kind: row.get(6)?,
                causation_id: row.get(7)?,
                status: row.get(8)?,
                job_id: row.get(9)?,
                lease_owner: row.get(10)?,
                lease_expires_at: row.get(11)?,
                fencing_token: row.get(12)?,
                host_generation: row.get(13)?,
                stop_epoch: row.get(14)?,
                global_stop_epoch: row.get(15)?,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
                completed_at: row.get(18)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
fn list_effects(conn: &rusqlite::Connection, id: &str) -> Result<Vec<ApiEffectRead>> {
    let mut stmt=conn.prepare("SELECT logical_effect_id,continuation_id,state,provider_identity,created_at,updated_at FROM execass_logical_effects WHERE delegation_id=?1 ORDER BY created_at,logical_effect_id")?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(ApiEffectRead {
                logical_effect_id: row.get(0)?,
                continuation_id: row.get(1)?,
                state: row.get(2)?,
                provider_identity: row.get(3)?,
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
fn list_recovery(conn: &rusqlite::Connection, id: &str) -> Result<Vec<ApiRecoveryRead>> {
    let mut stmt=conn.prepare("SELECT recovery_evaluation_id,logical_effect_id,evaluation_revision,directive,not_before_ms,evaluated_at FROM execass_recovery_evaluations WHERE delegation_id=?1 ORDER BY evaluated_at,recovery_evaluation_id")?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(ApiRecoveryRead {
                recovery_evaluation_id: row.get(0)?,
                logical_effect_id: row.get(1)?,
                evaluation_revision: row.get(2)?,
                directive: row.get(3)?,
                not_before_ms: row.get(4)?,
                evaluated_at: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
fn list_verifiers(conn: &rusqlite::Connection, id: &str) -> Result<Vec<ApiVerifierRead>> {
    let mut stmt=conn.prepare("SELECT verifier_result_id,criterion_id,result_revision,result,evidence_digest,verified_at FROM execass_verifier_results WHERE delegation_id=?1 ORDER BY criterion_id,result_revision")?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(ApiVerifierRead {
                verifier_result_id: row.get(0)?,
                criterion_id: row.get(1)?,
                result_revision: row.get(2)?,
                result: row.get(3)?,
                evidence_digest: row.get(4)?,
                verified_at: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn list_receipt_evidence(
    conn: &rusqlite::Connection,
    receipt_id: &str,
) -> Result<Vec<ApiReceiptEvidenceRead>> {
    let mut stmt = conn.prepare(
        "SELECT authority_kind,source_id,authoritative_revision,authority_link_id,observation_digest,deep_link FROM execass_receipt_evidence_refs WHERE receipt_id=?1 ORDER BY ordinal",
    )?;
    let evidence = stmt
        .query_map([receipt_id], |row| {
            Ok(ApiReceiptEvidenceRead {
                authority_kind: row.get(0)?,
                source_id: row.get(1)?,
                authoritative_revision: row.get(2)?,
                authority_link_id: row.get(3)?,
                observation_digest: row.get(4)?,
                deep_link: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(evidence)
}
