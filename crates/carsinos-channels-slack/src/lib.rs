use serde::{Deserialize, Serialize};

pub const SLACK_DEFAULT_CHUNK_LIMIT: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackAdapterConfig {
    pub require_mention_in_channels: bool,
    pub allowlisted_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackInboundMessage {
    pub channel_id: String,
    pub thread_ts: Option<String>,
    pub sender_id: String,
    pub text: String,
    pub is_direct_message: bool,
    pub mentions_bot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    Accept,
    Ignore(&'static str),
    Reject(&'static str),
}

pub fn route_message(config: &SlackAdapterConfig, message: &SlackInboundMessage) -> RouteDecision {
    if !config.allowlisted_user_ids.is_empty()
        && !config
            .allowlisted_user_ids
            .iter()
            .any(|id| id == &message.sender_id)
    {
        return RouteDecision::Reject("sender_not_allowlisted");
    }

    if !message.is_direct_message && config.require_mention_in_channels && !message.mentions_bot {
        return RouteDecision::Ignore("mention_required_in_channel");
    }

    if message.text.trim().is_empty() {
        return RouteDecision::Ignore("empty_message");
    }

    RouteDecision::Accept
}

pub fn session_key(message: &SlackInboundMessage) -> String {
    if message.is_direct_message {
        format!("slack:dm:{}", message.sender_id)
    } else if let Some(thread_ts) = &message.thread_ts {
        format!("slack:thread:{thread_ts}")
    } else {
        format!("slack:channel:{}", message.channel_id)
    }
}

pub fn split_outbound_chunks(text: &str, max_chars: usize) -> Vec<String> {
    let limit = max_chars.max(1);
    if text.chars().count() <= limit {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if current.chars().count() >= limit {
            chunks.push(current);
            current = String::new();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

pub fn approval_callback_payload(approval_id: &str, decision: &str) -> Option<String> {
    if approval_id.trim().is_empty() {
        return None;
    }
    if decision != "approve" && decision != "deny" {
        return None;
    }
    Some(format!("approval|{decision}|{approval_id}"))
}

pub fn parse_approval_callback_payload(payload: &str) -> Option<(String, String)> {
    let parts = payload.split('|').collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "approval" {
        return None;
    }
    let decision = parts[1];
    if decision != "approve" && decision != "deny" {
        return None;
    }
    let approval_id = parts[2].trim();
    if approval_id.is_empty() {
        return None;
    }
    Some((approval_id.to_string(), decision.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_without_mention_is_ignored() {
        let config = SlackAdapterConfig {
            require_mention_in_channels: true,
            allowlisted_user_ids: vec![],
        };
        let message = SlackInboundMessage {
            channel_id: "C123".to_string(),
            thread_ts: None,
            sender_id: "U1".to_string(),
            text: "hello".to_string(),
            is_direct_message: false,
            mentions_bot: false,
        };
        assert_eq!(
            route_message(&config, &message),
            RouteDecision::Ignore("mention_required_in_channel")
        );
    }

    #[test]
    fn allowlist_rejects_unknown_sender() {
        let config = SlackAdapterConfig {
            require_mention_in_channels: false,
            allowlisted_user_ids: vec!["U-ALLOWED".to_string()],
        };
        let message = SlackInboundMessage {
            channel_id: "D111".to_string(),
            thread_ts: None,
            sender_id: "U-DENIED".to_string(),
            text: "hi".to_string(),
            is_direct_message: true,
            mentions_bot: false,
        };
        assert_eq!(
            route_message(&config, &message),
            RouteDecision::Reject("sender_not_allowlisted")
        );
    }

    #[test]
    fn chunking_splits_long_messages() {
        let chunks = split_outbound_chunks("abcdefghij", 4);
        assert_eq!(chunks, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn approval_payload_round_trip() {
        let payload = approval_callback_payload("abc123", "approve").expect("payload");
        let parsed = parse_approval_callback_payload(&payload).expect("parsed");
        assert_eq!(parsed.0, "abc123");
        assert_eq!(parsed.1, "approve");
    }
}
