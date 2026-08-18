use serde::{Deserialize, Serialize};

pub const BLUEBUBBLES_DEFAULT_CHUNK_LIMIT: usize = 3500;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesAdapterConfig {
    pub require_reply_or_direct: bool,
    pub allowlisted_handles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueBubblesInboundMessage {
    pub chat_guid: String,
    pub sender_handle: String,
    pub text: String,
    pub is_group_chat: bool,
    pub is_direct_chat: bool,
    pub replies_to_agent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    Accept,
    Ignore(&'static str),
    Reject(&'static str),
}

pub fn route_message(
    config: &BlueBubblesAdapterConfig,
    message: &BlueBubblesInboundMessage,
) -> RouteDecision {
    if !config.allowlisted_handles.is_empty()
        && !config
            .allowlisted_handles
            .iter()
            .any(|value| value == &message.sender_handle)
    {
        return RouteDecision::Reject("sender_not_allowlisted");
    }

    if config.require_reply_or_direct && !message.is_direct_chat && !message.replies_to_agent {
        return RouteDecision::Ignore("reply_or_direct_required");
    }

    if message.text.trim().is_empty() {
        return RouteDecision::Ignore("empty_message");
    }

    RouteDecision::Accept
}

pub fn session_key(message: &BlueBubblesInboundMessage) -> String {
    if message.is_direct_chat {
        format!("imessage:dm:{}", message.sender_handle)
    } else {
        format!("imessage:chat:{}", message.chat_guid)
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
    fn group_without_reply_is_ignored_when_required() {
        let config = BlueBubblesAdapterConfig {
            require_reply_or_direct: true,
            allowlisted_handles: vec![],
        };
        let message = BlueBubblesInboundMessage {
            chat_guid: "chat-guid".to_string(),
            sender_handle: "+15551234".to_string(),
            text: "hello".to_string(),
            is_group_chat: true,
            is_direct_chat: false,
            replies_to_agent: false,
        };
        assert_eq!(
            route_message(&config, &message),
            RouteDecision::Ignore("reply_or_direct_required")
        );
    }

    #[test]
    fn allowlist_rejects_unknown_sender() {
        let config = BlueBubblesAdapterConfig {
            require_reply_or_direct: false,
            allowlisted_handles: vec!["+15559999".to_string()],
        };
        let message = BlueBubblesInboundMessage {
            chat_guid: "chat-guid".to_string(),
            sender_handle: "+18880000".to_string(),
            text: "hello".to_string(),
            is_group_chat: false,
            is_direct_chat: true,
            replies_to_agent: false,
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
        let payload = approval_callback_payload("abc123", "deny").expect("payload");
        let parsed = parse_approval_callback_payload(&payload).expect("parsed");
        assert_eq!(parsed.0, "abc123");
        assert_eq!(parsed.1, "deny");
    }
}
