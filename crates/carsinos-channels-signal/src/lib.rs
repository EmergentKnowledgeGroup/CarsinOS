use serde::{Deserialize, Serialize};

pub const SIGNAL_DEFAULT_CHUNK_LIMIT: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalAdapterConfig {
    pub require_mention_in_groups: bool,
    pub allowlisted_phone_numbers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalInboundMessage {
    pub chat_id: String,
    pub sender_phone: String,
    pub text: String,
    pub is_group_chat: bool,
    pub mentions_bot: bool,
    pub reply_to_bot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    Accept,
    Ignore(&'static str),
    Reject(&'static str),
}

pub fn route_message(
    config: &SignalAdapterConfig,
    message: &SignalInboundMessage,
) -> RouteDecision {
    if !config.allowlisted_phone_numbers.is_empty()
        && !config
            .allowlisted_phone_numbers
            .iter()
            .any(|value| value == &message.sender_phone)
    {
        return RouteDecision::Reject("sender_not_allowlisted");
    }

    if message.is_group_chat
        && config.require_mention_in_groups
        && !(message.mentions_bot || message.reply_to_bot)
    {
        return RouteDecision::Ignore("mention_required_in_group");
    }

    if message.text.trim().is_empty() {
        return RouteDecision::Ignore("empty_message");
    }

    RouteDecision::Accept
}

pub fn session_key(message: &SignalInboundMessage) -> String {
    if message.is_group_chat {
        format!("signal:group:{}", message.chat_id)
    } else {
        format!("signal:dm:{}", message.sender_phone)
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
    Some(format!("approval:{decision}:{approval_id}"))
}

pub fn parse_approval_callback_payload(payload: &str) -> Option<(String, String)> {
    let parts = payload.split(':').collect::<Vec<_>>();
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
    fn group_without_mention_is_ignored() {
        let config = SignalAdapterConfig {
            require_mention_in_groups: true,
            allowlisted_phone_numbers: vec![],
        };
        let message = SignalInboundMessage {
            chat_id: "group-id".to_string(),
            sender_phone: "+155500000".to_string(),
            text: "hello".to_string(),
            is_group_chat: true,
            mentions_bot: false,
            reply_to_bot: false,
        };
        assert_eq!(
            route_message(&config, &message),
            RouteDecision::Ignore("mention_required_in_group")
        );
    }

    #[test]
    fn allowlist_rejects_unknown_sender() {
        let config = SignalAdapterConfig {
            require_mention_in_groups: false,
            allowlisted_phone_numbers: vec!["+1999".to_string()],
        };
        let message = SignalInboundMessage {
            chat_id: "dm-id".to_string(),
            sender_phone: "+1888".to_string(),
            text: "hello".to_string(),
            is_group_chat: false,
            mentions_bot: false,
            reply_to_bot: false,
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
