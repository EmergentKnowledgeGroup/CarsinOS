//! Durable, per-consumer replay for the authenticated ExecAss event stream.
//!
//! Socket I/O deliberately happens outside this module. The gateway sends an
//! event first and then calls `commit_outbox_delivery`; a process death between
//! those steps replays a duplicate instead of losing an event.

use super::store::{immediate_transaction, ExecAssStore};
use super::types::OutboxEventRecord;
use super::validation::require_text;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxConsumerIdentity {
    /// Server-derived identity; callers must not use a raw client-provided ID.
    pub consumer_id: String,
    pub principal_id: String,
    /// A domain-separated digest of the stable client ID, never the raw ID.
    pub client_id_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxGapReason {
    StaleCursor,
    FutureCursor,
    CursorMismatch,
    ConsumerIdentityConflict,
    SequenceGap,
}

impl OutboxGapReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaleCursor => "stale_cursor",
            Self::FutureCursor => "future_cursor",
            Self::CursorMismatch => "cursor_mismatch",
            Self::ConsumerIdentityConflict => "consumer_identity_conflict",
            Self::SequenceGap => "sequence_gap",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxReplay {
    pub consumer_cursor: i64,
    pub head_global_sequence: i64,
    pub events: Vec<OutboxEventRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboxReplayOutcome {
    Replay(OutboxReplay),
    SummaryRefetchRequired {
        reason: OutboxGapReason,
        consumer_cursor: i64,
        requested_cursor: i64,
        head_global_sequence: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxDeliveryCommit<'a> {
    pub consumer: &'a OutboxConsumerIdentity,
    pub expected_cursor: i64,
    pub global_sequence: i64,
    pub published_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxDeliveryCommitOutcome {
    Committed,
    /// Another overlapping socket committed this exact event first.
    AlreadyCommitted,
    /// Another overlapping socket is ahead; this stream must stop and refetch.
    ConsumerAdvanced {
        consumer_cursor: i64,
    },
}

impl ExecAssStore {
    /// Reads immutable outbox rows after one authenticated consumer's exact
    /// durable cursor. `published_at` is intentionally not a replay filter:
    /// publication is global transport state, while delivery is per consumer.
    pub fn replay_outbox(
        &self,
        consumer: &OutboxConsumerIdentity,
        requested_cursor: i64,
    ) -> Result<OutboxReplayOutcome> {
        validate_consumer(consumer)?;
        if requested_cursor < 0 {
            return Ok(OutboxReplayOutcome::SummaryRefetchRequired {
                reason: OutboxGapReason::StaleCursor,
                consumer_cursor: 0,
                requested_cursor,
                head_global_sequence: self.outbox_head_sequence()?,
            });
        }
        let conn = self.connection()?;
        let head_global_sequence = outbox_head_sequence(&conn)?;
        let stored = conn
            .query_row(
                "SELECT principal_id,client_id_digest,last_global_sequence FROM execass_outbox_cursors WHERE consumer_id=?1",
                params![consumer.consumer_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?)),
            )
            .optional()
            .context("failed reading ExecAss outbox cursor")?;
        let (consumer_cursor, identity_conflict) = match stored {
            Some((principal_id, client_id_digest, cursor)) => (
                cursor,
                principal_id != consumer.principal_id
                    || client_id_digest != consumer.client_id_digest,
            ),
            None => (0, false),
        };
        if identity_conflict {
            return Ok(gap(
                OutboxGapReason::ConsumerIdentityConflict,
                consumer_cursor,
                requested_cursor,
                head_global_sequence,
            ));
        }
        if requested_cursor != consumer_cursor {
            let reason = if requested_cursor > head_global_sequence {
                OutboxGapReason::FutureCursor
            } else if requested_cursor < consumer_cursor {
                OutboxGapReason::StaleCursor
            } else {
                OutboxGapReason::CursorMismatch
            };
            return Ok(gap(
                reason,
                consumer_cursor,
                requested_cursor,
                head_global_sequence,
            ));
        }
        if requested_cursor > head_global_sequence {
            return Ok(gap(
                OutboxGapReason::FutureCursor,
                consumer_cursor,
                requested_cursor,
                head_global_sequence,
            ));
        }
        let events = super::rows::list_outbox_after(&conn, requested_cursor)?;
        let mut expected = requested_cursor;
        for event in &events {
            expected = expected
                .checked_add(1)
                .context("ExecAss outbox sequence overflow")?;
            if event.global_sequence != expected {
                return Ok(gap(
                    OutboxGapReason::SequenceGap,
                    consumer_cursor,
                    requested_cursor,
                    head_global_sequence,
                ));
            }
        }
        Ok(OutboxReplayOutcome::Replay(OutboxReplay {
            consumer_cursor,
            head_global_sequence,
            events,
        }))
    }

    pub fn outbox_head_sequence(&self) -> Result<i64> {
        outbox_head_sequence(&self.connection()?)
    }

    /// Test-only corruption probe. Production entry points reject a schema that
    /// has lost its guard trigger before reaching replay; this retains a direct
    /// proof that replay itself also refuses a gap if corruption bypasses that
    /// outer schema fence.
    #[cfg(test)]
    pub(crate) fn replay_outbox_after_schema_tamper_for_test(
        &self,
        consumer: &OutboxConsumerIdentity,
        requested_cursor: i64,
    ) -> Result<OutboxReplayOutcome> {
        validate_consumer(consumer)?;
        let conn = crate::open_sqlite_connection(&self.db_path)?;
        let head_global_sequence = outbox_head_sequence(&conn)?;
        let events = super::rows::list_outbox_after(&conn, requested_cursor)?;
        let mut expected = requested_cursor;
        for event in &events {
            expected = expected
                .checked_add(1)
                .context("ExecAss outbox sequence overflow")?;
            if event.global_sequence != expected {
                return Ok(gap(
                    OutboxGapReason::SequenceGap,
                    0,
                    requested_cursor,
                    head_global_sequence,
                ));
            }
        }
        Ok(OutboxReplayOutcome::Replay(OutboxReplay {
            consumer_cursor: 0,
            head_global_sequence,
            events,
        }))
    }

    /// Atomically records one post-send delivery and the global publication
    /// timestamp. This must be called only after the socket accepted the frame.
    pub fn commit_outbox_delivery(
        &self,
        command: OutboxDeliveryCommit<'_>,
    ) -> Result<OutboxDeliveryCommitOutcome> {
        validate_consumer(command.consumer)?;
        if command.expected_cursor < 0
            || command.global_sequence <= 0
            || command.published_at <= 0
            || command
                .expected_cursor
                .checked_add(1)
                .is_none_or(|expected| command.global_sequence != expected)
        {
            bail!("invalid ExecAss outbox delivery commit boundary");
        }
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        let stored = tx
            .query_row(
                "SELECT principal_id,client_id_digest,last_global_sequence,cursor_revision FROM execass_outbox_cursors WHERE consumer_id=?1",
                params![command.consumer.consumer_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?)),
            )
            .optional()?;
        match stored {
            Some((principal_id, client_id_digest, cursor, revision)) => {
                if principal_id != command.consumer.principal_id
                    || client_id_digest != command.consumer.client_id_digest
                {
                    bail!("ExecAss outbox consumer identity conflict");
                }
                if cursor == command.global_sequence {
                    tx.commit()
                        .context("failed committing idempotent ExecAss outbox delivery")?;
                    return Ok(OutboxDeliveryCommitOutcome::AlreadyCommitted);
                }
                if cursor > command.global_sequence {
                    tx.commit()
                        .context("failed committing advanced ExecAss outbox cursor read")?;
                    return Ok(OutboxDeliveryCommitOutcome::ConsumerAdvanced {
                        consumer_cursor: cursor,
                    });
                }
                if cursor != command.expected_cursor {
                    bail!("ExecAss outbox cursor changed before delivery commit");
                }
                let next_revision = revision
                    .checked_add(1)
                    .context("ExecAss outbox cursor revision overflow")?;
                let changed = tx.execute(
                    "UPDATE execass_outbox_cursors SET last_global_sequence=?1,cursor_revision=?2,updated_at=?3 WHERE consumer_id=?4",
                    params![command.global_sequence, next_revision, command.published_at, command.consumer.consumer_id],
                )?;
                if changed != 1 {
                    bail!("ExecAss outbox cursor update did not affect exactly one row");
                }
            }
            None => {
                if command.expected_cursor != 0 {
                    bail!("ExecAss outbox consumer cursor disappeared before first delivery");
                }
                tx.execute(
                    "INSERT INTO execass_outbox_cursors(consumer_id,principal_id,client_id_digest,last_global_sequence,cursor_revision,updated_at) VALUES(?1,?2,?3,?4,1,?5)",
                    params![command.consumer.consumer_id, command.consumer.principal_id, command.consumer.client_id_digest, command.global_sequence, command.published_at],
                )?;
            }
        }
        let changed = tx.execute(
            "UPDATE execass_outbox_events SET published_at=COALESCE(published_at,?1) WHERE global_sequence=?2",
            params![command.published_at, command.global_sequence],
        )?;
        if changed != 1 {
            bail!("ExecAss outbox event disappeared before delivery commit");
        }
        tx.commit()
            .context("failed committing ExecAss outbox delivery")?;
        Ok(OutboxDeliveryCommitOutcome::Committed)
    }
}

fn validate_consumer(consumer: &OutboxConsumerIdentity) -> Result<()> {
    for (field, value) in [
        ("outbox.consumer_id", consumer.consumer_id.as_str()),
        ("outbox.principal_id", consumer.principal_id.as_str()),
        (
            "outbox.client_id_digest",
            consumer.client_id_digest.as_str(),
        ),
    ] {
        require_text(field, value)?;
        if value.len() > 256 {
            bail!("{field} exceeds the bounded outbox transport identity length");
        }
    }
    Ok(())
}

fn outbox_head_sequence(conn: &rusqlite::Connection) -> Result<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(global_sequence),0) FROM execass_outbox_events",
        [],
        |row| row.get(0),
    )
    .context("failed reading ExecAss outbox head sequence")
}

fn gap(
    reason: OutboxGapReason,
    consumer_cursor: i64,
    requested_cursor: i64,
    head_global_sequence: i64,
) -> OutboxReplayOutcome {
    OutboxReplayOutcome::SummaryRefetchRequired {
        reason,
        consumer_cursor,
        requested_cursor,
        head_global_sequence,
    }
}
