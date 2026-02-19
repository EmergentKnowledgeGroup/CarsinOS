use serde::{Deserialize, Serialize};

pub const DISCORD_DEFAULT_CHUNK_LIMIT: usize = 1900;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordAdapterConfig {
    pub require_mention_in_guild_channels: bool,
    pub allowlisted_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordInboundMessage {
    pub guild_id: Option<String>,
    pub channel_id: String,
    pub thread_id: Option<String>,
    pub author_id: String,
    pub text: String,
    pub mentions_bot: bool,
    pub is_dm: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    Accept,
    Ignore(&'static str),
    Reject(&'static str),
}

pub fn route_message(
    config: &DiscordAdapterConfig,
    message: &DiscordInboundMessage,
) -> RouteDecision {
    if !config.allowlisted_user_ids.is_empty()
        && !config
            .allowlisted_user_ids
            .iter()
            .any(|id| id == &message.author_id)
    {
        return RouteDecision::Reject("sender_not_allowlisted");
    }

    if !message.is_dm && config.require_mention_in_guild_channels && !message.mentions_bot {
        return RouteDecision::Ignore("mention_required_in_guild_channel");
    }

    if message.text.trim().is_empty() {
        return RouteDecision::Ignore("empty_message");
    }

    RouteDecision::Accept
}

pub fn session_key(message: &DiscordInboundMessage) -> String {
    if message.is_dm {
        format!("discord:dm:{}", message.author_id)
    } else if let Some(thread_id) = &message.thread_id {
        format!("discord:thread:{thread_id}")
    } else {
        format!("discord:channel:{}", message.channel_id)
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

pub fn approval_custom_id(approval_id: &str, decision: &str) -> Option<String> {
    if approval_id.trim().is_empty() {
        return None;
    }
    if decision != "approve" && decision != "deny" {
        return None;
    }
    Some(format!("approval|{decision}|{approval_id}"))
}

pub fn parse_approval_custom_id(custom_id: &str) -> Option<(String, String)> {
    let parts = custom_id.split('|').collect::<Vec<_>>();
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
    fn guild_message_without_mention_is_ignored() {
        let config = DiscordAdapterConfig {
            require_mention_in_guild_channels: true,
            allowlisted_user_ids: vec![],
        };
        let message = DiscordInboundMessage {
            guild_id: Some("guild".to_string()),
            channel_id: "channel".to_string(),
            thread_id: None,
            author_id: "user".to_string(),
            text: "hello".to_string(),
            mentions_bot: false,
            is_dm: false,
        };
        assert_eq!(
            route_message(&config, &message),
            RouteDecision::Ignore("mention_required_in_guild_channel")
        );
    }

    #[test]
    fn thread_session_key_is_stable() {
        let message = DiscordInboundMessage {
            guild_id: Some("guild".to_string()),
            channel_id: "channel".to_string(),
            thread_id: Some("thread42".to_string()),
            author_id: "user".to_string(),
            text: "hi".to_string(),
            mentions_bot: true,
            is_dm: false,
        };
        assert_eq!(session_key(&message), "discord:thread:thread42");
    }

    #[test]
    fn chunking_splits_long_messages() {
        let chunks = split_outbound_chunks("abcdefghij", 3);
        assert_eq!(chunks, vec!["abc", "def", "ghi", "j"]);
    }

    #[test]
    fn approval_custom_id_round_trip() {
        let custom_id = approval_custom_id("abc123", "deny").expect("custom_id");
        let parsed = parse_approval_custom_id(&custom_id).expect("parsed");
        assert_eq!(parsed.0, "abc123");
        assert_eq!(parsed.1, "deny");
    }
}
