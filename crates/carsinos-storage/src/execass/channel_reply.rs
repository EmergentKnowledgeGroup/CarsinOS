//! Append-only provider-message bindings used only to resolve authenticated
//! reply correlations to one existing ExecAss delegation.

use super::store::{immediate_transaction, ExecAssStore};
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecAssChannelProvider {
    Telegram,
    Discord,
}

impl ExecAssChannelProvider {
    fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChannelReplyBinding {
    pub delegation_id: String,
    pub provider: ExecAssChannelProvider,
    pub authenticated_ingress: String,
    pub owner_credential_identity: String,
    pub conversation_id: String,
    pub outbound_message_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelReplyBindingRecord {
    pub binding_id: String,
    pub delegation_id: String,
    pub provider: ExecAssChannelProvider,
    pub authenticated_ingress: String,
    pub owner_credential_identity: String,
    pub conversation_id: String,
    pub outbound_message_id: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelReplyBindingWriteOutcome {
    Inserted(ChannelReplyBindingRecord),
    Replayed(ChannelReplyBindingRecord),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EligibleFollowUpTarget {
    pub delegation_id: String,
    pub state_revision: i64,
    pub plan_revision: i64,
}

impl ExecAssStore {
    /// Persist one outbound provider-message identity for later authenticated
    /// reply resolution. Exact retries converge; a tuple already bound to a
    /// different delegation is a conflict and never mutates history.
    pub fn record_channel_reply_binding(
        &self,
        input: &NewChannelReplyBinding,
    ) -> Result<ChannelReplyBindingWriteOutcome> {
        validate_binding(input)?;
        let binding_id = deterministic_binding_id(input);
        let mut conn = self.connection()?;
        let tx = immediate_transaction(&mut conn)?;
        if let Some(existing) = get_binding_by_id(&tx, &binding_id)? {
            if binding_matches_input(&existing, input) {
                return Ok(ChannelReplyBindingWriteOutcome::Replayed(existing));
            }
            bail!("channel reply binding identity conflicts with stored material");
        }
        let existing = get_binding_by_tuple(&tx, input)?;
        if let Some(existing) = existing {
            if binding_matches_input(&existing, input) {
                return Ok(ChannelReplyBindingWriteOutcome::Replayed(existing));
            }
            bail!("provider reply identity is already bound to another delegation");
        }
        let eligible = eligible_target(&tx, &input.delegation_id)?
            .context("channel reply binding requires an existing active delegation with a plan")?;
        if eligible.delegation_id != input.delegation_id {
            bail!("channel reply binding target mismatch");
        }
        tx.execute(
            "INSERT INTO execass_channel_reply_bindings (binding_id,delegation_id,provider,authenticated_ingress,owner_credential_identity,conversation_id,outbound_message_id,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![binding_id,input.delegation_id,input.provider.as_str(),input.authenticated_ingress,input.owner_credential_identity,input.conversation_id,input.outbound_message_id,input.created_at],
        )
        .context("persisting channel reply binding")?;
        let record = get_binding_by_id(&tx, &binding_id)?
            .context("inserted channel reply binding could not be reloaded")?;
        tx.commit()?;
        Ok(ChannelReplyBindingWriteOutcome::Inserted(record))
    }

    /// Resolve a trusted provider reply tuple. The canonical schema's UNIQUE
    /// constraint guarantees this returns zero or one active delegation.
    pub fn resolve_channel_reply_target(
        &self,
        provider: ExecAssChannelProvider,
        authenticated_ingress: &str,
        owner_credential_identity: &str,
        conversation_id: &str,
        outbound_message_id: &str,
    ) -> Result<Option<EligibleFollowUpTarget>> {
        validate_identity("authenticated_ingress", authenticated_ingress, 256)?;
        validate_identity("owner_credential_identity", owner_credential_identity, 256)?;
        validate_identity("conversation_id", conversation_id, 256)?;
        validate_identity("outbound_message_id", outbound_message_id, 256)?;
        let conn = self.connection()?;
        conn.query_row(
            "SELECT d.delegation_id,d.state_revision,d.current_plan_revision FROM execass_channel_reply_bindings b JOIN execass_delegations d ON d.delegation_id=b.delegation_id WHERE b.provider=?1 AND b.authenticated_ingress=?2 AND b.owner_credential_identity=?3 AND b.conversation_id=?4 AND b.outbound_message_id=?5 AND d.phase NOT IN ('completed','partially_completed','failed') AND d.current_plan_revision IS NOT NULL",
            params![provider.as_str(),authenticated_ingress,owner_credential_identity,conversation_id,outbound_message_id],
            |row| Ok(EligibleFollowUpTarget { delegation_id: row.get(0)?, state_revision: row.get(1)?, plan_revision: row.get(2)? }),
        )
        .optional()
        .map_err(Into::into)
    }

    /// Read the immutable provider-message binding without considering the
    /// delegation's current eligibility. This is intentionally separate from
    /// `resolve_channel_reply_target`: callers use it only to recognize an
    /// exact prior authenticated ingress replay after the delegation has
    /// advanced to a terminal state.
    pub fn read_channel_reply_binding(
        &self,
        provider: ExecAssChannelProvider,
        authenticated_ingress: &str,
        owner_credential_identity: &str,
        conversation_id: &str,
        outbound_message_id: &str,
    ) -> Result<Option<ChannelReplyBindingRecord>> {
        validate_identity("authenticated_ingress", authenticated_ingress, 256)?;
        validate_identity("owner_credential_identity", owner_credential_identity, 256)?;
        validate_identity("conversation_id", conversation_id, 256)?;
        validate_identity("outbound_message_id", outbound_message_id, 256)?;
        let conn = self.connection()?;
        conn.query_row(
            "SELECT binding_id,delegation_id,provider,authenticated_ingress,owner_credential_identity,conversation_id,outbound_message_id,created_at FROM execass_channel_reply_bindings WHERE provider=?1 AND authenticated_ingress=?2 AND owner_credential_identity=?3 AND conversation_id=?4 AND outbound_message_id=?5",
            params![provider.as_str(),authenticated_ingress,owner_credential_identity,conversation_id,outbound_message_id],
            binding_from_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn resolve_explicit_follow_up_target(
        &self,
        delegation_id: &str,
    ) -> Result<Option<EligibleFollowUpTarget>> {
        validate_identity("delegation_id", delegation_id, 256)?;
        let conn = self.connection()?;
        eligible_target(&conn, delegation_id)
    }
}

fn validate_binding(input: &NewChannelReplyBinding) -> Result<()> {
    validate_identity("delegation_id", &input.delegation_id, 256)?;
    validate_identity("authenticated_ingress", &input.authenticated_ingress, 256)?;
    validate_identity(
        "owner_credential_identity",
        &input.owner_credential_identity,
        256,
    )?;
    validate_identity("conversation_id", &input.conversation_id, 256)?;
    validate_identity("outbound_message_id", &input.outbound_message_id, 256)?;
    if input.created_at <= 0 {
        bail!("channel reply binding created_at must be positive");
    }
    Ok(())
}

fn validate_identity(field: &str, value: &str, limit: usize) -> Result<()> {
    if value.is_empty()
        || value.trim() != value
        || value.len() > limit
        || value.chars().any(char::is_control)
    {
        bail!("invalid channel reply {field}");
    }
    Ok(())
}

fn deterministic_binding_id(input: &NewChannelReplyBinding) -> String {
    let mut digest = Sha256::new();
    for value in [
        b"carsinos.execass.channel_reply_binding.v1".as_slice(),
        input.provider.as_str().as_bytes(),
        input.authenticated_ingress.as_bytes(),
        input.owner_credential_identity.as_bytes(),
        input.conversation_id.as_bytes(),
        input.outbound_message_id.as_bytes(),
    ] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value);
    }
    format!("execass-reply-{:x}", digest.finalize())
}

fn binding_matches_input(
    record: &ChannelReplyBindingRecord,
    input: &NewChannelReplyBinding,
) -> bool {
    record.delegation_id == input.delegation_id
        && record.provider == input.provider
        && record.authenticated_ingress == input.authenticated_ingress
        && record.owner_credential_identity == input.owner_credential_identity
        && record.conversation_id == input.conversation_id
        && record.outbound_message_id == input.outbound_message_id
}

fn get_binding_by_id(
    conn: &rusqlite::Connection,
    binding_id: &str,
) -> Result<Option<ChannelReplyBindingRecord>> {
    conn.query_row(
        "SELECT binding_id,delegation_id,provider,authenticated_ingress,owner_credential_identity,conversation_id,outbound_message_id,created_at FROM execass_channel_reply_bindings WHERE binding_id=?1",
        [binding_id],
        binding_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn get_binding_by_tuple(
    conn: &rusqlite::Connection,
    input: &NewChannelReplyBinding,
) -> Result<Option<ChannelReplyBindingRecord>> {
    conn.query_row(
        "SELECT binding_id,delegation_id,provider,authenticated_ingress,owner_credential_identity,conversation_id,outbound_message_id,created_at FROM execass_channel_reply_bindings WHERE provider=?1 AND authenticated_ingress=?2 AND owner_credential_identity=?3 AND conversation_id=?4 AND outbound_message_id=?5",
        params![input.provider.as_str(),input.authenticated_ingress,input.owner_credential_identity,input.conversation_id,input.outbound_message_id],
        binding_from_row,
    )
    .optional()
    .map_err(Into::into)
}

fn binding_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ChannelReplyBindingRecord> {
    let provider: String = row.get(2)?;
    let provider = match provider.as_str() {
        "telegram" => ExecAssChannelProvider::Telegram,
        "discord" => ExecAssChannelProvider::Discord,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(ChannelReplyBindingRecord {
        binding_id: row.get(0)?,
        delegation_id: row.get(1)?,
        provider,
        authenticated_ingress: row.get(3)?,
        owner_credential_identity: row.get(4)?,
        conversation_id: row.get(5)?,
        outbound_message_id: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn eligible_target(
    conn: &rusqlite::Connection,
    delegation_id: &str,
) -> Result<Option<EligibleFollowUpTarget>> {
    conn.query_row(
        "SELECT delegation_id,state_revision,current_plan_revision FROM execass_delegations WHERE delegation_id=?1 AND phase NOT IN ('completed','partially_completed','failed') AND current_plan_revision IS NOT NULL",
        [delegation_id],
        |row| Ok(EligibleFollowUpTarget { delegation_id: row.get(0)?, state_revision: row.get(1)?, plan_revision: row.get(2)? }),
    )
    .optional()
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{init_execass_fresh_root, AppPaths};

    fn fixture() -> (tempfile::TempDir, ExecAssStore) {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_root(temp.path());
        init_execass_fresh_root(&paths).unwrap();
        let store = ExecAssStore::open(&paths).unwrap();
        let conn = store.connection().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO execass_authority_provenance (
              authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
              channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
              policy_revision,evidence_digest,created_at
            ) VALUES ('authority-1','human_remote','telegram:owner-1','telegram-long-poll',
                      'authenticated-telegram-provider-event','corr-1','original_request','{}',
                      1,'evidence-1',1);
            INSERT INTO execass_delegations (
              delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,
              ingress_credential_identity,source_correlation_id,ingress_idempotency_key,
              classifier_version,classifier_reasons_json,phase,run_control,state_revision,
              policy_revision,effective_authority_json,authority_provenance_id,created_at,updated_at
            ) VALUES ('delegation-1','test','{}','telegram-long-poll','telegram:owner-1',
                      'corr-1','idem-1','v1','[]','accepted','running',1,1,'{}','authority-1',1,1);
            INSERT INTO execass_plans (
              plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,
              plan_summary,resolved_leaf_manifest_json,manifest_digest,
              created_by_authority_provenance_id,created_at
            ) VALUES ('plan-1','delegation-1',1,1,1,'test plan','[]','manifest-1','authority-1',1);
            UPDATE execass_delegations SET current_plan_revision=1,state_revision=2 WHERE delegation_id='delegation-1';
            "#,
        )
        .unwrap();
        (temp, store)
    }

    fn binding() -> NewChannelReplyBinding {
        NewChannelReplyBinding {
            delegation_id: "delegation-1".into(),
            provider: ExecAssChannelProvider::Telegram,
            authenticated_ingress: "telegram-long-poll".into(),
            owner_credential_identity: "telegram:owner-1".into(),
            conversation_id: "chat-1".into(),
            outbound_message_id: "77".into(),
            created_at: 100,
        }
    }

    #[test]
    fn exact_binding_replays_and_resolves_one_active_delegation() {
        let (_temp, store) = fixture();
        let first = store.record_channel_reply_binding(&binding()).unwrap();
        let second = store.record_channel_reply_binding(&binding()).unwrap();
        let ChannelReplyBindingWriteOutcome::Inserted(first) = first else {
            panic!("first binding must insert")
        };
        let ChannelReplyBindingWriteOutcome::Replayed(second) = second else {
            panic!("exact binding retry must replay")
        };
        assert_eq!(first, second);
        assert_eq!(
            store
                .resolve_channel_reply_target(
                    ExecAssChannelProvider::Telegram,
                    "telegram-long-poll",
                    "telegram:owner-1",
                    "chat-1",
                    "77",
                )
                .unwrap(),
            Some(EligibleFollowUpTarget {
                delegation_id: "delegation-1".into(),
                state_revision: 2,
                plan_revision: 1,
            })
        );
        assert!(store
            .resolve_channel_reply_target(
                ExecAssChannelProvider::Telegram,
                "telegram-long-poll",
                "telegram:other-owner",
                "chat-1",
                "77",
            )
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .read_channel_reply_binding(
                    ExecAssChannelProvider::Telegram,
                    "telegram-long-poll",
                    "telegram:owner-1",
                    "chat-1",
                    "77",
                )
                .unwrap()
                .map(|binding| binding.delegation_id),
            Some("delegation-1".into()),
            "terminal state hides the active target but never erases the immutable replay binding"
        );
    }

    #[test]
    fn conflict_invalid_identity_and_terminal_target_cause_zero_retargeting() {
        let (_temp, store) = fixture();
        store.record_channel_reply_binding(&binding()).unwrap();
        let conn = store.connection().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO execass_authority_provenance (
              authority_provenance_id,actor_type,credential_identity,authenticated_ingress,
              channel_assurance,source_correlation_id,authority_kind,normalized_scope_json,
              policy_revision,evidence_digest,created_at
            ) VALUES ('authority-2','human_remote','telegram:owner-1','telegram-long-poll',
                      'authenticated-telegram-provider-event','corr-2','original_request','{}',
                      1,'evidence-2',2);
            INSERT INTO execass_delegations (
              delegation_id,normalized_original_intent,intake_evidence_json,ingress_source,
              ingress_credential_identity,source_correlation_id,ingress_idempotency_key,
              classifier_version,classifier_reasons_json,phase,run_control,state_revision,
              policy_revision,effective_authority_json,authority_provenance_id,created_at,updated_at
            ) VALUES ('delegation-2','test','{}','telegram-long-poll','telegram:owner-1',
                      'corr-2','idem-2','v1','[]','accepted','running',1,1,'{}','authority-2',2,2);
            INSERT INTO execass_plans (
              plan_id,delegation_id,plan_revision,based_on_delegation_revision,policy_revision,
              plan_summary,resolved_leaf_manifest_json,manifest_digest,
              created_by_authority_provenance_id,created_at
            ) VALUES ('plan-2','delegation-2',1,1,1,'test plan','[]','manifest-2','authority-2',2);
            UPDATE execass_delegations SET current_plan_revision=1,state_revision=2 WHERE delegation_id='delegation-2';
            "#,
        )
        .unwrap();
        let mut conflict = binding();
        conflict.delegation_id = "delegation-2".into();
        assert!(store.record_channel_reply_binding(&conflict).is_err());
        assert_eq!(
            store
                .resolve_channel_reply_target(
                    ExecAssChannelProvider::Telegram,
                    "telegram-long-poll",
                    "telegram:owner-1",
                    "chat-1",
                    "77",
                )
                .unwrap()
                .unwrap()
                .delegation_id,
            "delegation-1"
        );

        let mut invalid = binding();
        invalid.outbound_message_id = "bad\nmessage".into();
        assert!(store.record_channel_reply_binding(&invalid).is_err());
        conn.execute(
            "UPDATE execass_delegations SET phase='completed',state_revision=3,terminal_at=3,updated_at=3 WHERE delegation_id='delegation-1'",
            [],
        )
        .unwrap();
        assert!(store
            .resolve_explicit_follow_up_target("delegation-1")
            .unwrap()
            .is_none());
        assert!(store
            .resolve_channel_reply_target(
                ExecAssChannelProvider::Telegram,
                "telegram-long-poll",
                "telegram:owner-1",
                "chat-1",
                "77",
            )
            .unwrap()
            .is_none());
    }

    #[test]
    fn persisted_binding_is_immutable_and_undeletable() {
        let (_temp, store) = fixture();
        let outcome = store.record_channel_reply_binding(&binding()).unwrap();
        let record = match outcome {
            ChannelReplyBindingWriteOutcome::Inserted(record)
            | ChannelReplyBindingWriteOutcome::Replayed(record) => record,
        };
        let conn = store.connection().unwrap();
        assert!(conn
            .execute(
                "UPDATE execass_channel_reply_bindings SET outbound_message_id='other' WHERE binding_id=?1",
                [&record.binding_id],
            )
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM execass_channel_reply_bindings WHERE binding_id=?1",
                [&record.binding_id],
            )
            .is_err());
    }
}
