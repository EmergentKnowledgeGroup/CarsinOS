//! Exact executive-summary delivery, acknowledgement, and notification state.
//!
//! This module records what was actually rendered. It deliberately does not
//! publish the outbox, send a channel message, or touch WebSocket transport.

use super::projection::canonical_delivered_items;
use super::redaction::ReceiptRedactor;
use super::rows::insert_outbox;
use super::store::{immediate_transaction, ExecAssStore};
use super::types::*;
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use rusqlite::{params, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::str::FromStr;

const DELIVERY_CURSOR_VERSION: &str = "carsinos.execass.summary_cursor.v1";
const ACK_DIGEST_VERSION: &str = "carsinos.execass.summary_ack.v1";
const REMINDER_INTERVAL_MS: i64 = 60 * 60 * 1000;
const MAX_REMINDERS: u8 = 3;

impl ExecAssStore {
    /// Records the exact five-pane item set rendered from one authoritative
    /// projection. Replays are byte-for-byte identical; changed requests are
    /// conflicts, never a second delivery.
    pub fn record_summary_delivery(
        &self,
        projection: &ExecAssExecutiveProjection,
        command: &SummaryDeliveryCommand,
    ) -> Result<SummaryDeliveryOutcome> {
        let canonical_items = canonical_delivery_command(projection, command)?;
        let cursor = displayed_cursor(command);
        let expected = SummaryDeliveryRecord {
            delivery_id: command.delivery_id.clone(),
            displayed_cursor: cursor,
            projection_version: command.projection_version.clone(),
            through_global_sequence: command.through_global_sequence,
            item_set_digest: command.item_set_digest.clone(),
            request_identity: command.request_identity.clone(),
            delivered_at: command.delivered_at,
            items: canonical_items,
        };

        let mut connection = self.connection()?;
        let tx = immediate_transaction(&mut connection)?;
        if let Some(existing) = find_delivery(&tx, &command.delivery_id)? {
            tx.commit()?;
            return Ok(if existing == expected {
                SummaryDeliveryOutcome::Replayed(existing)
            } else {
                SummaryDeliveryOutcome::Conflict
            });
        }
        if let Some(existing) = find_delivery_by_cursor(&tx, &expected.displayed_cursor)? {
            tx.commit()?;
            return Ok(if existing == expected {
                SummaryDeliveryOutcome::Replayed(existing)
            } else {
                SummaryDeliveryOutcome::Conflict
            });
        }
        insert_delivery(&tx, &expected)?;
        tx.commit()?;
        Ok(SummaryDeliveryOutcome::Recorded(expected))
    }

    /// Acknowledges one and only one previously rendered exact set. A repeat
    /// using a different idempotency key is safe only when it is the same ack.
    pub fn acknowledge_summary_delivery(
        &self,
        command: &SummaryAcknowledgementCommand,
    ) -> Result<SummaryAcknowledgementOutcome> {
        validate_ack_command(command)?;
        let supplied = canonical_items(&command.items)?;
        let digest = acknowledgement_digest(&supplied);
        let mut connection = self.connection()?;
        let tx = immediate_transaction(&mut connection)?;
        if let Some(existing) = find_ack_by_idempotency(&tx, &command.idempotency_key)? {
            tx.commit()?;
            return Ok(
                if existing.0.delivery_id == command.delivery_id
                    && existing.0.displayed_cursor == command.displayed_cursor
                    && existing.1 == digest
                {
                    SummaryAcknowledgementOutcome::Replayed(existing.0)
                } else {
                    SummaryAcknowledgementOutcome::Conflict
                },
            );
        }
        let Some(delivery) = find_delivery(&tx, &command.delivery_id)? else {
            tx.commit()?;
            return Ok(SummaryAcknowledgementOutcome::NotDelivered);
        };
        if delivery.displayed_cursor != command.displayed_cursor {
            tx.commit()?;
            return Ok(SummaryAcknowledgementOutcome::NotDelivered);
        }
        if supplied != delivery.items {
            tx.commit()?;
            return Ok(SummaryAcknowledgementOutcome::Conflict);
        }
        if let Some(existing) = find_ack_for_delivery(&tx, &delivery.delivery_id)? {
            tx.commit()?;
            return Ok(if existing.1 == digest {
                SummaryAcknowledgementOutcome::Replayed(existing.0)
            } else {
                SummaryAcknowledgementOutcome::Conflict
            });
        }
        let record = SummaryAcknowledgementRecord {
            acknowledgement_id: deterministic_id("ack", &[&delivery.delivery_id, &digest]),
            delivery_id: delivery.delivery_id,
            displayed_cursor: delivery.displayed_cursor,
            acknowledged_at: command.acknowledged_at,
        };
        tx.execute(
            r#"INSERT INTO execass_summary_acknowledgements(
                 acknowledgement_id,delivery_id,displayed_cursor,acknowledged_items_digest,
                 idempotency_key,acknowledged_at
               ) VALUES(?1,?2,?3,?4,?5,?6)"#,
            params![
                record.acknowledgement_id,
                record.delivery_id,
                record.displayed_cursor,
                digest,
                command.idempotency_key,
                record.acknowledged_at,
            ],
        )?;
        tx.commit()?;
        Ok(SummaryAcknowledgementOutcome::Acknowledged(record))
    }

    /// HTTP acknowledgement facade: the opaque displayed cursor selects the
    /// server-recorded delivery identity. Callers never choose a delivery row.
    pub fn acknowledge_api_summary_cursor(
        &self,
        displayed_cursor: &str,
        idempotency_key: &str,
        acknowledged_at: i64,
        items: Vec<SummaryDeliveredItem>,
    ) -> Result<SummaryAcknowledgementOutcome> {
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let Some(delivery) = find_delivery_by_cursor(&tx, displayed_cursor)? else {
            return Ok(SummaryAcknowledgementOutcome::NotDelivered);
        };
        tx.commit()?;
        drop(conn);
        self.acknowledge_summary_delivery(&SummaryAcknowledgementCommand {
            delivery_id: delivery.delivery_id,
            displayed_cursor: displayed_cursor.to_owned(),
            idempotency_key: idempotency_key.to_owned(),
            acknowledged_at,
            items,
        })
    }

    /// Persists a deduplicated notification schedule and its un-published
    /// durable outbox fact. Sending the notification is intentionally outside
    /// this storage facade.
    pub fn schedule_notification(
        &self,
        command: &NotificationScheduleCommand,
        redactor: &ReceiptRedactor,
    ) -> Result<NotificationScheduleOutcome> {
        validate_notification_command(command, redactor)?;
        let mut connection = self.connection()?;
        let tx = immediate_transaction(&mut connection)?;

        if let Some(existing) = find_notification_by_id(&tx, &command.notification_id)? {
            tx.commit()?;
            return notification_exact_replay_outcome(existing, command);
        }
        if let Some(existing) = find_notification_by_idempotency(&tx, &command.idempotency_key)? {
            tx.commit()?;
            return notification_exact_replay_outcome(existing, command);
        }
        if let Some(existing) = find_notification_by_dedupe(&tx, command)? {
            tx.commit()?;
            return notification_dedupe_replay_outcome(existing, command);
        }

        let scheduled_at =
            defer_for_quiet_hours(command.scheduled_at, command.quiet_hours.as_ref())?;
        let Some(binding) = current_notification_binding(&tx, command, scheduled_at)? else {
            tx.commit()?;
            return Ok(NotificationScheduleOutcome::NotActionable);
        };
        let next_reminder_at = if binding.completion_assessment_id.is_some() {
            None
        } else {
            bounded_next_reminder(scheduled_at, binding.deadline_ms)
        };
        let quiet_hours_json = quiet_hours_json(command.quiet_hours.as_ref())?;
        let safe_payload_json = String::from_utf8(command.safe_payload.canonical_bytes())
            .map_err(|_| anyhow::anyhow!("sensitive content could not be stored safely"))?;
        let outbox_event_id = insert_notification_outbox(&tx, command, &binding)?;
        tx.execute(
            r#"INSERT INTO execass_notifications(
                 notification_id,attention_id,completion_assessment_id,outbox_event_id,delegation_id,decision_id,reason_revision,
                 attention_variant,reason,channel,status,safe_payload_json,scheduled_at,
                 requested_at,next_reminder_at,quiet_hours_json,reminder_count,last_reminded_at,updated_at,idempotency_key
               ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'scheduled',?11,?12,?13,?14,?15,0,NULL,?12,?16)"#,
            params![
                command.notification_id,
                binding.attention_id,
                binding.completion_assessment_id,
                outbox_event_id,
                command.delegation_id,
                command.decision_id,
                command.reason_revision,
                binding.attention_variant,
                command.reason.as_str(),
                command.channel,
                safe_payload_json,
                scheduled_at,
                command.scheduled_at,
                next_reminder_at,
                quiet_hours_json,
                command.idempotency_key,
            ],
        )?;
        let record = NotificationScheduleRecord {
            notification_id: command.notification_id.clone(),
            scheduled_at,
            next_reminder_at,
            reminder_count: 0,
            last_reminded_at: None,
            cancelled: false,
        };
        tx.commit()?;
        Ok(NotificationScheduleOutcome::Scheduled(record))
    }

    /// Advances only durable reminder state. It does not dispatch a message.
    /// A stale, resolved, superseded, or expired binding is cancelled instead.
    pub fn advance_notification_reminder(
        &self,
        notification_id: &str,
        trusted_now: i64,
    ) -> Result<NotificationScheduleOutcome> {
        require_text("notification_id", notification_id)?;
        if trusted_now <= 0 {
            bail!("notification reminder requires a positive trusted clock");
        }
        let mut connection = self.connection()?;
        let tx = immediate_transaction(&mut connection)?;
        let Some(stored) = find_notification_by_id(&tx, notification_id)? else {
            tx.commit()?;
            return Ok(NotificationScheduleOutcome::NotActionable);
        };
        if stored.cancelled || !stored.actionable_at(&tx, trusted_now)? {
            let cancelled = cancel_notification(&tx, stored, trusted_now)?;
            tx.commit()?;
            return Ok(NotificationScheduleOutcome::Cancelled(cancelled));
        }
        if stored.reminder_count >= MAX_REMINDERS
            || stored
                .next_reminder_at
                .is_none_or(|next| next > trusted_now)
        {
            tx.commit()?;
            return Ok(NotificationScheduleOutcome::Scheduled(stored.record()));
        }
        let next_count = stored.reminder_count + 1;
        let next = if next_count >= MAX_REMINDERS {
            None
        } else {
            bounded_next_reminder(
                defer_for_quiet_hours(trusted_now, stored.quiet_hours.as_ref())?,
                stored.deadline_ms,
            )
        };
        tx.execute(
            "UPDATE execass_notifications SET reminder_count=?2,last_reminded_at=?3,next_reminder_at=?4,updated_at=?3 WHERE notification_id=?1",
            params![notification_id, next_count, trusted_now, next],
        )?;
        let record = NotificationScheduleRecord {
            notification_id: notification_id.to_owned(),
            scheduled_at: stored.scheduled_at,
            next_reminder_at: next,
            reminder_count: next_count,
            last_reminded_at: Some(trusted_now),
            cancelled: false,
        };
        tx.commit()?;
        Ok(NotificationScheduleOutcome::Scheduled(record))
    }
}

#[derive(Clone)]
struct AttentionBinding {
    attention_id: Option<String>,
    completion_assessment_id: Option<String>,
    attention_variant: Option<String>,
    deadline_ms: Option<i64>,
    delegation_state_revision: i64,
}

#[derive(Clone)]
struct StoredNotification {
    notification_id: String,
    attention_id: Option<String>,
    completion_assessment_id: Option<String>,
    delegation_id: String,
    decision_id: Option<String>,
    reason_revision: i64,
    channel: String,
    reason: String,
    safe_payload_json: String,
    requested_at: i64,
    scheduled_at: i64,
    next_reminder_at: Option<i64>,
    quiet_hours: Option<QuietHoursPolicy>,
    reminder_count: u8,
    last_reminded_at: Option<i64>,
    cancelled: bool,
    idempotency_key: String,
    deadline_ms: Option<i64>,
}

impl StoredNotification {
    fn record(&self) -> NotificationScheduleRecord {
        NotificationScheduleRecord {
            notification_id: self.notification_id.clone(),
            scheduled_at: self.scheduled_at,
            next_reminder_at: self.next_reminder_at,
            reminder_count: self.reminder_count,
            last_reminded_at: self.last_reminded_at,
            cancelled: self.cancelled,
        }
    }

    fn actionable_at(&self, tx: &Transaction<'_>, now: i64) -> Result<bool> {
        let Some(attention_id) = &self.attention_id else {
            return Ok(self.completion_assessment_id.is_some());
        };
        let count: i64 = tx.query_row(
            r#"SELECT COUNT(*) FROM execass_attention_items a
               JOIN execass_delegations d ON d.delegation_id=a.delegation_id
               LEFT JOIN execass_decisions decision ON decision.decision_id=a.decision_id
               LEFT JOIN execass_confirmation_challenges challenge ON challenge.decision_id=decision.decision_id
               WHERE a.attention_id=?1 AND a.delegation_id=?2 AND a.decision_id IS ?3
                 AND a.status='actionable' AND d.state_revision=a.delegation_revision
                 AND d.phase NOT IN ('completed','partially_completed','failed')
                 AND (a.decision_id IS NULL OR (d.pending_decision_id=a.decision_id
                      AND decision.status='pending' AND decision.decision_revision=?4
                      AND (challenge.challenge_id IS NULL OR (challenge.status='pending' AND challenge.expires_at>?5))))"#,
            params![attention_id, self.delegation_id, self.decision_id, self.reason_revision, now],
            |row| row.get(0),
        )?;
        Ok(count == 1)
    }
}

fn canonical_delivery_command(
    projection: &ExecAssExecutiveProjection,
    command: &SummaryDeliveryCommand,
) -> Result<Vec<SummaryDeliveredItem>> {
    for (field, value) in [
        ("delivery_id", command.delivery_id.as_str()),
        ("request_identity", command.request_identity.as_str()),
        ("projection_version", command.projection_version.as_str()),
        ("item_set_digest", command.item_set_digest.as_str()),
    ] {
        require_text(field, value)?;
    }
    if command.delivered_at <= 0 || command.through_global_sequence < 0 {
        bail!("summary delivery has an invalid timestamp or cursor boundary");
    }
    if command.projection_version != projection.projection_version
        || command.through_global_sequence != projection.boundary.through_global_sequence
        || command.item_set_digest != projection.boundary.item_set_digest
        || super::projection::item_set_digest(projection)? != projection.boundary.item_set_digest
    {
        bail!("summary delivery does not bind the exact projection boundary");
    }
    let expected = canonical_delivered_items(projection)?;
    if canonical_items(&command.items)? != expected {
        bail!("summary delivery does not contain the exact rendered item set");
    }
    Ok(expected)
}

fn validate_ack_command(command: &SummaryAcknowledgementCommand) -> Result<()> {
    for (field, value) in [
        ("delivery_id", command.delivery_id.as_str()),
        ("displayed_cursor", command.displayed_cursor.as_str()),
        ("idempotency_key", command.idempotency_key.as_str()),
    ] {
        require_text(field, value)?;
    }
    if command.acknowledged_at <= 0 {
        bail!("summary acknowledgement requires a positive timestamp");
    }
    canonical_items(&command.items).map(|_| ())
}

fn canonical_items(items: &[SummaryDeliveredItem]) -> Result<Vec<SummaryDeliveredItem>> {
    let mut sorted = items.to_vec();
    for item in &sorted {
        require_text("summary item_id", &item.item_id)?;
        if item.revision <= 0
            || !item
                .item_id
                .starts_with(&format!("{}:", item.projection_kind.as_str()))
        {
            bail!("summary delivered item is invalid");
        }
    }
    sorted.sort();
    let unique = sorted
        .iter()
        .map(|item| (&item.projection_kind, &item.item_id))
        .collect::<BTreeSet<_>>();
    if unique.len() != sorted.len() {
        bail!("summary delivered item set contains duplicates");
    }
    Ok(sorted)
}

fn displayed_cursor(command: &SummaryDeliveryCommand) -> String {
    deterministic_id(
        "summary-cursor",
        &[
            DELIVERY_CURSOR_VERSION,
            &command.projection_version,
            &command.through_global_sequence.to_string(),
            &command.item_set_digest,
            &command.delivered_at.to_string(),
            &command.request_identity,
        ],
    )
}

fn acknowledgement_digest(items: &[SummaryDeliveredItem]) -> String {
    let mut digest = Sha256::new();
    digest.update(ACK_DIGEST_VERSION.as_bytes());
    for item in items {
        digest.update(b"\0");
        digest.update(item.projection_kind.as_str().as_bytes());
        digest.update(b"\0");
        digest.update(item.item_id.as_bytes());
        digest.update(b"\0");
        digest.update(item.revision.to_be_bytes());
    }
    format!("sha256:{:x}", digest.finalize())
}

fn deterministic_id(prefix: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(prefix.as_bytes());
    for part in parts {
        digest.update(b"\0");
        digest.update(part.as_bytes());
    }
    format!("{prefix}-{:x}", digest.finalize())
}

fn find_delivery(tx: &Transaction<'_>, delivery_id: &str) -> Result<Option<SummaryDeliveryRecord>> {
    find_delivery_where(tx, "delivery_id=?1", params![delivery_id])
}

fn find_delivery_by_cursor(
    tx: &Transaction<'_>,
    cursor: &str,
) -> Result<Option<SummaryDeliveryRecord>> {
    find_delivery_where(tx, "displayed_cursor=?1", params![cursor])
}

fn find_delivery_where<P: rusqlite::Params>(
    tx: &Transaction<'_>,
    predicate: &str,
    params: P,
) -> Result<Option<SummaryDeliveryRecord>> {
    let sql = format!(
        "SELECT delivery_id,displayed_cursor,projection_version,through_global_sequence,item_set_digest,request_correlation_id,delivered_at FROM execass_summary_deliveries WHERE {predicate}"
    );
    let record = tx
        .query_row(&sql, params, |row| {
            Ok(SummaryDeliveryRecord {
                delivery_id: row.get(0)?,
                displayed_cursor: row.get(1)?,
                projection_version: row.get(2)?,
                through_global_sequence: row.get(3)?,
                item_set_digest: row.get(4)?,
                request_identity: row.get(5)?,
                delivered_at: row.get(6)?,
                items: Vec::new(),
            })
        })
        .optional()?;
    record
        .map(|mut record| {
            record.items = delivery_items(tx, &record.delivery_id)?;
            Ok(record)
        })
        .transpose()
}

fn delivery_items(tx: &Transaction<'_>, delivery_id: &str) -> Result<Vec<SummaryDeliveredItem>> {
    let mut statement = tx.prepare(
        "SELECT item_id,item_revision,projection_kind FROM execass_summary_delivery_items WHERE delivery_id=?1 ORDER BY projection_kind,item_id,item_revision",
    )?;
    let items = statement
        .query_map([delivery_id], |row| {
            Ok(SummaryDeliveredItem {
                item_id: row.get(0)?,
                revision: row.get(1)?,
                projection_kind: SummaryProjectionKind::parse(&row.get::<_, String>(2)?)
                    .ok_or_else(|| rusqlite::Error::InvalidQuery)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    canonical_items(&items)
}

fn insert_delivery(tx: &Transaction<'_>, record: &SummaryDeliveryRecord) -> Result<()> {
    tx.execute(
        r#"INSERT INTO execass_summary_deliveries(
             delivery_id,displayed_cursor,projection_version,through_global_sequence,item_set_digest,item_count,
             request_correlation_id,delivered_at
           ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)"#,
        params![record.delivery_id, record.displayed_cursor, record.projection_version,
            record.through_global_sequence, record.item_set_digest, record.items.len() as i64,
            record.request_identity, record.delivered_at],
    )?;
    for item in &record.items {
        tx.execute(
            "INSERT INTO execass_summary_delivery_items(delivery_id,item_id,item_revision,projection_kind) VALUES(?1,?2,?3,?4)",
            params![record.delivery_id, item.item_id, item.revision, item.projection_kind.as_str()],
        )?;
    }
    Ok(())
}

fn find_ack_for_delivery(
    tx: &Transaction<'_>,
    delivery_id: &str,
) -> Result<Option<(SummaryAcknowledgementRecord, String)>> {
    find_ack_where(tx, "delivery_id=?1", params![delivery_id])
}

fn find_ack_by_idempotency(
    tx: &Transaction<'_>,
    key: &str,
) -> Result<Option<(SummaryAcknowledgementRecord, String)>> {
    find_ack_where(tx, "idempotency_key=?1", params![key])
}

fn find_ack_where<P: rusqlite::Params>(
    tx: &Transaction<'_>,
    predicate: &str,
    params: P,
) -> Result<Option<(SummaryAcknowledgementRecord, String)>> {
    let sql = format!(
        "SELECT acknowledgement_id,delivery_id,displayed_cursor,acknowledged_items_digest,acknowledged_at FROM execass_summary_acknowledgements WHERE {predicate}"
    );
    tx.query_row(&sql, params, |row| {
        Ok((
            SummaryAcknowledgementRecord {
                acknowledgement_id: row.get(0)?,
                delivery_id: row.get(1)?,
                displayed_cursor: row.get(2)?,
                acknowledged_at: row.get(4)?,
            },
            row.get(3)?,
        ))
    })
    .optional()
    .map_err(Into::into)
}

fn validate_notification_command(
    command: &NotificationScheduleCommand,
    redactor: &ReceiptRedactor,
) -> Result<()> {
    for (field, value) in [
        ("notification_id", command.notification_id.as_str()),
        ("delegation_id", command.delegation_id.as_str()),
        ("channel", command.channel.as_str()),
        ("idempotency_key", command.idempotency_key.as_str()),
    ] {
        require_text(field, value)?;
    }
    match &command.source {
        NotificationSource::Attention { attention_id } => {
            require_text("attention_id", attention_id)?
        }
        NotificationSource::Completion {
            completion_assessment_id,
            ..
        } => require_text("completion_assessment_id", completion_assessment_id)?,
    }
    if command.reason_revision <= 0 || command.scheduled_at <= 0 {
        bail!("notification has an invalid revision or timestamp");
    }
    redactor.reject_sensitive_bytes(command.reason.as_str().as_bytes())?;
    redactor.reject_sensitive_bytes(&command.safe_payload.canonical_bytes())?;
    let _ = quiet_hours_json(command.quiet_hours.as_ref())?;
    Ok(())
}

fn current_notification_binding(
    tx: &Transaction<'_>,
    command: &NotificationScheduleCommand,
    effective_scheduled_at: i64,
) -> Result<Option<AttentionBinding>> {
    match &command.source {
        NotificationSource::Attention { attention_id } => {
            current_attention_binding(tx, command, attention_id, effective_scheduled_at)
        }
        NotificationSource::Completion {
            completion_assessment_id,
            completion_enabled,
        } => {
            if !completion_enabled {
                return Ok(None);
            }
            current_completion_binding(tx, command, completion_assessment_id)
        }
    }
}

fn current_attention_binding(
    tx: &Transaction<'_>,
    command: &NotificationScheduleCommand,
    attention_id: &str,
    effective_scheduled_at: i64,
) -> Result<Option<AttentionBinding>> {
    tx.query_row(
        r#"SELECT a.kind,d.state_revision,
                  CASE WHEN a.decision_id IS NULL THEN a.delegation_revision ELSE decision.decision_revision END,
                  challenge.expires_at
           FROM execass_attention_items a
           JOIN execass_delegations d ON d.delegation_id=a.delegation_id
           LEFT JOIN execass_decisions decision ON decision.decision_id=a.decision_id
           LEFT JOIN execass_confirmation_challenges challenge ON challenge.decision_id=decision.decision_id
           WHERE a.attention_id=?1 AND a.delegation_id=?2 AND a.decision_id IS ?3
             AND a.status='actionable' AND d.state_revision=a.delegation_revision
             AND d.phase NOT IN ('completed','partially_completed','failed')
             AND (a.decision_id IS NULL OR (
                  d.pending_decision_id=a.decision_id AND decision.status='pending'
                  AND decision.delegation_id=a.delegation_id
                  AND decision.delegation_revision=a.delegation_revision
                  AND ((decision.decision_kind='dangerous_action_confirmation'
                       AND (SELECT COUNT(*) FROM execass_confirmation_challenges exact WHERE exact.decision_id=decision.decision_id)=1
                       AND challenge.status='pending' AND challenge.expires_at>?4)
                       OR (decision.decision_kind!='dangerous_action_confirmation'
                           AND challenge.challenge_id IS NULL))))"#,
        params![attention_id, command.delegation_id, command.decision_id, effective_scheduled_at],
        |row| Ok((AttentionBinding { attention_id: Some(attention_id.to_owned()), completion_assessment_id: None, attention_variant: Some(row.get(0)?), delegation_state_revision: row.get(1)?, deadline_ms: row.get(3)? }, row.get::<_, i64>(2)?)),
    ).optional().map_err(Into::into).map(|binding| {
        if binding.as_ref().is_some_and(|(_, reason_revision)| *reason_revision != command.reason_revision) {
            None
        } else { binding.map(|(binding, _)| binding) }
    })
}

fn current_completion_binding(
    tx: &Transaction<'_>,
    command: &NotificationScheduleCommand,
    assessment_id: &str,
) -> Result<Option<AttentionBinding>> {
    tx.query_row(
        r#"SELECT assessment.assessment_revision,delegation.state_revision
           FROM execass_completion_assessments assessment
           JOIN execass_delegations delegation ON delegation.delegation_id=assessment.delegation_id
           WHERE assessment.assessment_id=?1 AND assessment.delegation_id=?2
             AND delegation.phase=assessment.terminal_phase
             AND delegation.terminal_at IS NOT NULL
             AND delegation.completion_assessment_json=assessment.assessment_json
             AND assessment.no_remaining_path=1
             AND (SELECT COUNT(*) FROM execass_receipts receipt
                        WHERE receipt.delegation_id=assessment.delegation_id
                          AND receipt.subject_kind='completion_assessment'
                          AND receipt.subject_id=assessment.assessment_id
                          AND receipt.subject_revision=assessment.assessment_revision)=1
             AND NOT EXISTS(SELECT 1 FROM execass_completion_assessments newer
                            WHERE newer.delegation_id=assessment.delegation_id
                              AND newer.assessment_revision>assessment.assessment_revision)"#,
        params![assessment_id, command.delegation_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )
    .optional()
    .map_err(Into::into)
    .map(|revision| {
        revision
            .filter(|(reason_revision, _)| *reason_revision == command.reason_revision)
            .map(|(_, delegation_state_revision)| AttentionBinding {
                attention_id: None,
                completion_assessment_id: Some(assessment_id.to_owned()),
                attention_variant: None,
                delegation_state_revision,
                deadline_ms: None,
            })
    })
}

fn quiet_hours_json(policy: Option<&QuietHoursPolicy>) -> Result<Option<String>> {
    let Some(policy) = policy else {
        return Ok(None);
    };
    validate_quiet_hours(policy)?;
    Ok(Some(serde_json::to_string(
        &json!({"timezone":policy.timezone,"start_minute":policy.start_minute,"end_minute":policy.end_minute}),
    )?))
}

fn parse_quiet_hours(value: Option<String>) -> Result<Option<QuietHoursPolicy>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let policy: QuietHoursPolicy =
        serde_json::from_str(&value).context("stored quiet-hours policy is malformed")?;
    validate_quiet_hours(&policy)?;
    Ok(Some(policy))
}

fn validate_quiet_hours(policy: &QuietHoursPolicy) -> Result<Tz> {
    if policy.start_minute >= 24 * 60
        || policy.end_minute >= 24 * 60
        || policy.start_minute == policy.end_minute
    {
        bail!("quiet hours must be a non-empty local interval");
    }
    Tz::from_str(&policy.timezone).context("invalid IANA quiet-hours timezone")
}

pub(super) fn defer_for_quiet_hours(at_ms: i64, policy: Option<&QuietHoursPolicy>) -> Result<i64> {
    let Some(policy) = policy else {
        return Ok(at_ms);
    };
    let tz = validate_quiet_hours(policy)?;
    let utc = DateTime::<Utc>::from_timestamp_millis(at_ms)
        .context("notification timestamp is invalid")?;
    let local = utc.with_timezone(&tz);
    let minute = local.hour() as u16 * 60 + local.minute() as u16;
    let in_quiet = if policy.start_minute < policy.end_minute {
        minute >= policy.start_minute && minute < policy.end_minute
    } else {
        minute >= policy.start_minute || minute < policy.end_minute
    };
    if !in_quiet {
        return Ok(at_ms);
    }
    let end_date = if policy.start_minute > policy.end_minute && minute >= policy.start_minute {
        local
            .date_naive()
            .succ_opt()
            .context("quiet-hours local date overflow")?
    } else {
        local.date_naive()
    };
    resolve_quiet_end(tz, end_date, policy.end_minute)
}

fn resolve_quiet_end(tz: Tz, date: NaiveDate, minute: u16) -> Result<i64> {
    let time = NaiveTime::from_hms_opt((minute / 60) as u32, (minute % 60) as u32, 0)
        .context("quiet-hours local time is invalid")?;
    let mut local = date.and_time(time);
    for _ in 0..=(24 * 60) {
        match tz.from_local_datetime(&local) {
            chrono::LocalResult::None => {
                local = local
                    .checked_add_signed(Duration::minutes(1))
                    .context("quiet-hours DST advance overflowed")?
            }
            chrono::LocalResult::Single(instant) => return Ok(instant.timestamp_millis()),
            chrono::LocalResult::Ambiguous(_, later) => return Ok(later.timestamp_millis()),
        }
    }
    bail!("quiet-hours end did not resolve within 24 hours")
}

fn bounded_next_reminder(scheduled_at: i64, deadline: Option<i64>) -> Option<i64> {
    let candidate = scheduled_at.checked_add(REMINDER_INTERVAL_MS)?;
    match deadline {
        Some(deadline) if deadline <= scheduled_at => None,
        Some(deadline) => Some(candidate.min(deadline)),
        None => Some(candidate),
    }
}

fn find_notification_by_id(tx: &Transaction<'_>, id: &str) -> Result<Option<StoredNotification>> {
    find_notification_where(tx, "n.notification_id=?1", params![id])
}
fn find_notification_by_idempotency(
    tx: &Transaction<'_>,
    key: &str,
) -> Result<Option<StoredNotification>> {
    find_notification_where(tx, "n.idempotency_key=?1", params![key])
}
fn find_notification_by_dedupe(
    tx: &Transaction<'_>,
    command: &NotificationScheduleCommand,
) -> Result<Option<StoredNotification>> {
    find_notification_where(
        tx,
        "n.delegation_id=?1 AND n.decision_id IS ?2 AND n.reason_revision=?3 AND n.channel=?4",
        params![
            command.delegation_id,
            command.decision_id,
            command.reason_revision,
            command.channel
        ],
    )
}
fn find_notification_where<P: rusqlite::Params>(
    tx: &Transaction<'_>,
    predicate: &str,
    params: P,
) -> Result<Option<StoredNotification>> {
    let sql = format!(
        r#"SELECT n.notification_id,n.attention_id,n.completion_assessment_id,n.delegation_id,n.decision_id,n.reason_revision,n.channel,n.reason,n.safe_payload_json,n.requested_at,n.scheduled_at,n.next_reminder_at,n.quiet_hours_json,n.reminder_count,n.last_reminded_at,n.status,n.idempotency_key,
                              challenge.expires_at
                       FROM execass_notifications n LEFT JOIN execass_decisions decision ON decision.decision_id=n.decision_id
                       LEFT JOIN execass_confirmation_challenges challenge ON challenge.decision_id=decision.decision_id
                       WHERE {predicate}"#
    );
    tx.query_row(&sql, params, |row| {
        Ok(StoredNotification {
            notification_id: row.get(0)?,
            attention_id: row.get(1)?,
            completion_assessment_id: row.get(2)?,
            delegation_id: row.get(3)?,
            decision_id: row.get(4)?,
            reason_revision: row.get(5)?,
            channel: row.get(6)?,
            reason: row.get(7)?,
            safe_payload_json: row.get(8)?,
            requested_at: row.get(9)?,
            scheduled_at: row.get(10)?,
            next_reminder_at: row.get(11)?,
            quiet_hours: parse_quiet_hours(row.get(12)?).map_err(to_sql_error)?,
            reminder_count: row.get(13)?,
            last_reminded_at: row.get(14)?,
            cancelled: row.get::<_, String>(15)? == "cancelled",
            idempotency_key: row.get(16)?,
            deadline_ms: row.get(17)?,
        })
    })
    .optional()
    .map_err(Into::into)
}
fn to_sql_error(_error: anyhow::Error) -> rusqlite::Error {
    rusqlite::Error::InvalidQuery
}

fn notification_exact_replay_outcome(
    existing: StoredNotification,
    command: &NotificationScheduleCommand,
) -> Result<NotificationScheduleOutcome> {
    let payload = String::from_utf8(command.safe_payload.canonical_bytes())
        .map_err(|_| anyhow::anyhow!("sensitive content could not be stored safely"))?;
    if !notification_source_matches(&existing, command)
        || existing.notification_id != command.notification_id
        || existing.idempotency_key != command.idempotency_key
        || existing.delegation_id != command.delegation_id
        || existing.decision_id != command.decision_id
        || existing.reason_revision != command.reason_revision
        || existing.channel != command.channel
        || existing.reason != command.reason.as_str()
        || existing.safe_payload_json != payload
        || existing.requested_at != command.scheduled_at
        || existing.quiet_hours != command.quiet_hours
    {
        return Ok(NotificationScheduleOutcome::Conflict);
    }
    Ok(if existing.cancelled {
        NotificationScheduleOutcome::Cancelled(existing.record())
    } else {
        NotificationScheduleOutcome::Replayed(existing.record())
    })
}

fn notification_dedupe_replay_outcome(
    existing: StoredNotification,
    command: &NotificationScheduleCommand,
) -> Result<NotificationScheduleOutcome> {
    let payload = String::from_utf8(command.safe_payload.canonical_bytes())
        .map_err(|_| anyhow::anyhow!("sensitive content could not be stored safely"))?;
    if !notification_source_matches(&existing, command)
        || existing.delegation_id != command.delegation_id
        || existing.decision_id != command.decision_id
        || existing.reason_revision != command.reason_revision
        || existing.channel != command.channel
        || existing.reason != command.reason.as_str()
        || existing.safe_payload_json != payload
        || existing.quiet_hours != command.quiet_hours
    {
        return Ok(NotificationScheduleOutcome::Conflict);
    }
    Ok(if existing.cancelled {
        NotificationScheduleOutcome::Cancelled(existing.record())
    } else {
        NotificationScheduleOutcome::Replayed(existing.record())
    })
}

fn notification_source_matches(
    existing: &StoredNotification,
    command: &NotificationScheduleCommand,
) -> bool {
    match &command.source {
        NotificationSource::Attention { attention_id } => {
            existing.attention_id.as_deref() == Some(attention_id)
                && existing.completion_assessment_id.is_none()
        }
        NotificationSource::Completion {
            completion_assessment_id,
            completion_enabled,
        } => {
            *completion_enabled
                && existing.attention_id.is_none()
                && existing.completion_assessment_id.as_deref() == Some(completion_assessment_id)
        }
    }
}

fn cancel_notification(
    tx: &Transaction<'_>,
    stored: StoredNotification,
    now: i64,
) -> Result<NotificationScheduleRecord> {
    tx.execute("UPDATE execass_notifications SET status='cancelled',next_reminder_at=NULL,updated_at=?2 WHERE notification_id=?1", params![stored.notification_id, now])?;
    Ok(NotificationScheduleRecord {
        notification_id: stored.notification_id,
        scheduled_at: stored.scheduled_at,
        next_reminder_at: None,
        reminder_count: stored.reminder_count,
        last_reminded_at: stored.last_reminded_at,
        cancelled: true,
    })
}

fn insert_notification_outbox(
    tx: &Transaction<'_>,
    command: &NotificationScheduleCommand,
    binding: &AttentionBinding,
) -> Result<String> {
    let event_id = deterministic_id("notification-scheduled", &[&command.notification_id]);
    let safe_payload_json = serde_json::to_string(&json!({
        "summary": command.reason.as_str(),
        "delegation_id": command.delegation_id,
        "decision_id": command.decision_id,
        "notification_id": command.notification_id,
        "attention_id": binding.attention_id,
        "completion_assessment_id": binding.completion_assessment_id,
        "reason_revision": command.reason_revision,
        "channel": command.channel
    }))?;
    insert_outbox(
        tx,
        &NewOutboxEvent {
            event_id: event_id.clone(),
            event_name: OutboxEventName::NotificationScheduled,
            aggregate_id: command.delegation_id.clone(),
            aggregate_revision: binding.delegation_state_revision,
            correlation_id: command.idempotency_key.clone(),
            causation_id: command.notification_id.clone(),
            occurred_at: command.scheduled_at,
            safe_payload_json,
            duplicate_identity: deterministic_id(
                "notification-dedupe",
                &[
                    &command.delegation_id,
                    command.decision_id.as_deref().unwrap_or(""),
                    &command.reason_revision.to_string(),
                    &command.channel,
                ],
            ),
        },
    )?;
    Ok(event_id)
}
