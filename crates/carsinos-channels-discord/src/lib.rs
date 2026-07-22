use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use url::form_urlencoded;

pub const DISCORD_DEFAULT_CHUNK_LIMIT: usize = 1900;
pub const DISCORD_DEFAULT_API_BASE_URL: &str = "https://discord.com/api/v10";
const DISCORD_RETRY_ATTEMPT_HEADER: &str = "X-CarsinOS-Retry-Attempt";

#[derive(Debug, Clone)]
pub struct DiscordTransportConfig {
    pub api_base_url: String,
    pub bot_token: String,
    pub timeout_ms: u64,
    pub retry_attempts: usize,
}

impl Default for DiscordTransportConfig {
    fn default() -> Self {
        Self {
            api_base_url: DISCORD_DEFAULT_API_BASE_URL.to_string(),
            bot_token: String::new(),
            timeout_ms: 5_000,
            retry_attempts: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordTransportOutboundRequest {
    pub channel_id: String,
    pub content: String,
    #[serde(default)]
    pub reply_to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordPinMessageRequest {
    pub channel_id: String,
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordReactionRequest {
    pub channel_id: String,
    pub message_id: String,
    pub emoji: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordSendMessageResult {
    pub message_id: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordTransportInboundAuthor {
    pub id: String,
    #[serde(default)]
    pub bot: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordTransportInboundMention {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordTransportMessageReference {
    #[serde(
        default,
        deserialize_with = "deserialize_optional_discord_message_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscordTransportInboundMessage {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub content: String,
    pub author: DiscordTransportInboundAuthor,
    #[serde(default)]
    pub mentions: Vec<DiscordTransportInboundMention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<DiscordTransportMessageReference>,
}

impl DiscordTransportInboundMessage {
    /// Returns only Discord's provider-supplied reply-message identifier.
    ///
    /// This is correlation evidence, not an authorization or ExecAss attachment decision.
    pub fn reply_to_message_id(&self) -> Option<&str> {
        self.message_reference
            .as_ref()
            .and_then(|reference| reference.message_id.as_deref())
    }
}

fn deserialize_optional_discord_message_id<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    let value = value
        .map(|message_id| message_id.trim().to_string())
        .filter(|message_id| !message_id.is_empty());
    if value.as_deref().is_some_and(|message_id| {
        message_id.len() > 128 || message_id.chars().any(char::is_control)
    }) {
        return Err(serde::de::Error::custom(
            "Discord reply message_id is invalid",
        ));
    }
    Ok(value)
}

#[derive(Debug, Clone)]
struct DiscordTransportError {
    message: String,
    retryable: bool,
}

#[derive(Debug, Clone)]
pub struct DiscordTransportClient {
    config: DiscordTransportConfig,
    agent: ureq::Agent,
}

impl DiscordTransportClient {
    pub fn new(config: DiscordTransportConfig) -> Result<Self> {
        let base_url = config.api_base_url.trim();
        if config.bot_token.trim().is_empty() {
            return Err(anyhow!("discord bot_token must not be empty"));
        }
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            return Err(anyhow!(
                "discord api_base_url must start with http:// or https://"
            ));
        }
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_millis(config.timeout_ms.max(250)))
            .build();
        Ok(Self { config, agent })
    }

    pub fn config(&self) -> &DiscordTransportConfig {
        &self.config
    }

    pub fn send_message_with_retry(
        &self,
        request: &DiscordTransportOutboundRequest,
    ) -> Result<DiscordSendMessageResult> {
        let channel_id = request.channel_id.trim();
        if channel_id.is_empty() {
            return Err(anyhow!("discord outbound channel_id must not be empty"));
        }
        if request.content.trim().is_empty() {
            return Err(anyhow!("discord outbound content must not be empty"));
        }

        let payload = if let Some(reply_to_message_id) = request
            .reply_to_message_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            json!({
                "content": request.content,
                "message_reference": {
                    "message_id": reply_to_message_id,
                    "channel_id": channel_id,
                    "fail_if_not_exists": false
                }
            })
        } else {
            json!({ "content": request.content })
        };
        let response = self.call_with_retry(channel_id, &payload)?;
        let message_id = response
            .get("id")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .ok_or_else(|| anyhow!("discord create-message response missing id"))?;
        let response_channel_id = response
            .get("channel_id")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string())
            .unwrap_or_else(|| channel_id.to_string());
        Ok(DiscordSendMessageResult {
            message_id,
            channel_id: response_channel_id,
        })
    }

    pub fn get_channel_messages_with_retry(
        &self,
        channel_id: &str,
        limit: usize,
    ) -> Result<Vec<DiscordTransportInboundMessage>> {
        let channel_id = channel_id.trim();
        if channel_id.is_empty() {
            return Err(anyhow!("discord inbound channel_id must not be empty"));
        }
        let limit = limit.clamp(1, 100);
        let max_attempts = self.config.retry_attempts.max(1);
        let mut last_error: Option<DiscordTransportError> = None;
        for attempt in 1..=max_attempts {
            match self.get_channel_messages_once(channel_id, limit, attempt) {
                Ok(messages) => return Ok(messages),
                Err(err) => {
                    let should_retry = err.retryable && attempt < max_attempts;
                    last_error = Some(err);
                    if should_retry {
                        std::thread::sleep(Duration::from_millis(100 * attempt as u64));
                        continue;
                    }
                    break;
                }
            }
        }

        let final_error = last_error.unwrap_or(DiscordTransportError {
            message: "unknown discord transport failure".to_string(),
            retryable: false,
        });
        Err(anyhow!(
            "discord list-messages failed: {}",
            final_error.message
        ))
    }

    pub fn pin_message_with_retry(&self, request: &DiscordPinMessageRequest) -> Result<()> {
        let channel_id = request.channel_id.trim();
        let message_id = request.message_id.trim();
        if channel_id.is_empty() {
            return Err(anyhow!("discord pin channel_id must not be empty"));
        }
        if message_id.is_empty() {
            return Err(anyhow!("discord pin message_id must not be empty"));
        }
        let max_attempts = self.config.retry_attempts.max(1);
        let mut last_error: Option<DiscordTransportError> = None;
        for attempt in 1..=max_attempts {
            match self.pin_message_once(channel_id, message_id, attempt) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let should_retry = err.retryable && attempt < max_attempts;
                    last_error = Some(err);
                    if should_retry {
                        std::thread::sleep(Duration::from_millis(100 * attempt as u64));
                        continue;
                    }
                    break;
                }
            }
        }
        let final_error = last_error.unwrap_or(DiscordTransportError {
            message: "unknown discord transport failure".to_string(),
            retryable: false,
        });
        Err(anyhow!(
            "discord pin-message failed: {}",
            final_error.message
        ))
    }

    pub fn add_reaction_with_retry(&self, request: &DiscordReactionRequest) -> Result<()> {
        let channel_id = request.channel_id.trim();
        let message_id = request.message_id.trim();
        let emoji = request.emoji.trim();
        if channel_id.is_empty() {
            return Err(anyhow!("discord reaction channel_id must not be empty"));
        }
        if message_id.is_empty() {
            return Err(anyhow!("discord reaction message_id must not be empty"));
        }
        if emoji.is_empty() {
            return Err(anyhow!("discord reaction emoji must not be empty"));
        }
        let max_attempts = self.config.retry_attempts.max(1);
        let mut last_error: Option<DiscordTransportError> = None;
        for attempt in 1..=max_attempts {
            match self.add_reaction_once(channel_id, message_id, emoji, attempt) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    let should_retry = err.retryable && attempt < max_attempts;
                    last_error = Some(err);
                    if should_retry {
                        std::thread::sleep(Duration::from_millis(100 * attempt as u64));
                        continue;
                    }
                    break;
                }
            }
        }
        let final_error = last_error.unwrap_or(DiscordTransportError {
            message: "unknown discord transport failure".to_string(),
            retryable: false,
        });
        Err(anyhow!(
            "discord add-reaction failed: {}",
            final_error.message
        ))
    }

    fn call_with_retry(&self, channel_id: &str, payload: &Value) -> Result<Value> {
        let max_attempts = self.config.retry_attempts.max(1);
        let mut last_error: Option<DiscordTransportError> = None;
        for attempt in 1..=max_attempts {
            match self.call_once(channel_id, payload, attempt) {
                Ok(response) => return Ok(response),
                Err(err) => {
                    let should_retry = err.retryable && attempt < max_attempts;
                    last_error = Some(err);
                    if should_retry {
                        std::thread::sleep(Duration::from_millis(100 * attempt as u64));
                        continue;
                    }
                    break;
                }
            }
        }

        let final_error = last_error.unwrap_or(DiscordTransportError {
            message: "unknown discord transport failure".to_string(),
            retryable: false,
        });
        Err(anyhow!(
            "discord create-message failed: {}",
            final_error.message
        ))
    }

    fn call_once(
        &self,
        channel_id: &str,
        payload: &Value,
        attempt: usize,
    ) -> std::result::Result<Value, DiscordTransportError> {
        let url = format!(
            "{}/channels/{}/messages",
            self.config.api_base_url.trim_end_matches('/'),
            channel_id
        );
        let response = self
            .agent
            .post(&url)
            .set("Accept", "application/json")
            .set(
                "Authorization",
                &format!("Bot {}", self.config.bot_token.trim()),
            )
            .set(DISCORD_RETRY_ATTEMPT_HEADER, &attempt.to_string())
            .send_json(payload.clone());
        match response {
            Ok(response) => response
                .into_json::<Value>()
                .map_err(|err| DiscordTransportError {
                    message: format!("invalid discord JSON response: {}", err),
                    retryable: false,
                }),
            Err(ureq::Error::Status(status, response)) => {
                let body = response
                    .into_string()
                    .unwrap_or_else(|_| "<unreadable body>".to_string());
                let retryable = status == 429 || status >= 500;
                Err(DiscordTransportError {
                    message: format!("HTTP {}: {}", status, body),
                    retryable,
                })
            }
            Err(ureq::Error::Transport(err)) => Err(DiscordTransportError {
                message: format!("transport error: {}", err),
                retryable: true,
            }),
        }
    }

    fn get_channel_messages_once(
        &self,
        channel_id: &str,
        limit: usize,
        attempt: usize,
    ) -> std::result::Result<Vec<DiscordTransportInboundMessage>, DiscordTransportError> {
        let url = format!(
            "{}/channels/{}/messages?limit={}",
            self.config.api_base_url.trim_end_matches('/'),
            channel_id,
            limit
        );
        let response = self
            .agent
            .get(&url)
            .set("Accept", "application/json")
            .set(
                "Authorization",
                &format!("Bot {}", self.config.bot_token.trim()),
            )
            .set(DISCORD_RETRY_ATTEMPT_HEADER, &attempt.to_string())
            .call();
        match response {
            Ok(response) => response
                .into_json::<Vec<DiscordTransportInboundMessage>>()
                .map_err(|err| DiscordTransportError {
                    message: format!("invalid discord JSON response: {}", err),
                    retryable: false,
                }),
            Err(ureq::Error::Status(status, response)) => {
                let body = response
                    .into_string()
                    .unwrap_or_else(|_| "<unreadable body>".to_string());
                let retryable = status == 429 || status >= 500;
                Err(DiscordTransportError {
                    message: format!("HTTP {}: {}", status, body),
                    retryable,
                })
            }
            Err(ureq::Error::Transport(err)) => Err(DiscordTransportError {
                message: format!("transport error: {}", err),
                retryable: true,
            }),
        }
    }

    fn pin_message_once(
        &self,
        channel_id: &str,
        message_id: &str,
        attempt: usize,
    ) -> std::result::Result<(), DiscordTransportError> {
        let url = format!(
            "{}/channels/{}/pins/{}",
            self.config.api_base_url.trim_end_matches('/'),
            channel_id,
            message_id
        );
        let response = self
            .agent
            .put(&url)
            .set("Accept", "application/json")
            .set(
                "Authorization",
                &format!("Bot {}", self.config.bot_token.trim()),
            )
            .set(DISCORD_RETRY_ATTEMPT_HEADER, &attempt.to_string())
            .call();
        match response {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(status, response)) => {
                let body = response
                    .into_string()
                    .unwrap_or_else(|_| "<unreadable body>".to_string());
                let retryable = status == 429 || status >= 500;
                Err(DiscordTransportError {
                    message: format!("HTTP {}: {}", status, body),
                    retryable,
                })
            }
            Err(ureq::Error::Transport(err)) => Err(DiscordTransportError {
                message: format!("transport error: {}", err),
                retryable: true,
            }),
        }
    }

    fn add_reaction_once(
        &self,
        channel_id: &str,
        message_id: &str,
        emoji: &str,
        attempt: usize,
    ) -> std::result::Result<(), DiscordTransportError> {
        let emoji_encoded = form_urlencoded::byte_serialize(emoji.as_bytes()).collect::<String>();
        let url = format!(
            "{}/channels/{}/messages/{}/reactions/{}/@me",
            self.config.api_base_url.trim_end_matches('/'),
            channel_id,
            message_id,
            emoji_encoded
        );
        let response = self
            .agent
            .put(&url)
            .set("Accept", "application/json")
            .set(
                "Authorization",
                &format!("Bot {}", self.config.bot_token.trim()),
            )
            .set(DISCORD_RETRY_ATTEMPT_HEADER, &attempt.to_string())
            .call();
        match response {
            Ok(_) => Ok(()),
            Err(ureq::Error::Status(status, response)) => {
                let body = response
                    .into_string()
                    .unwrap_or_else(|_| "<unreadable body>".to_string());
                let retryable = status == 429 || status >= 500;
                Err(DiscordTransportError {
                    message: format!("HTTP {}: {}", status, body),
                    retryable,
                })
            }
            Err(ureq::Error::Transport(err)) => Err(DiscordTransportError {
                message: format!("transport error: {}", err),
                retryable: true,
            }),
        }
    }
}

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
    use httpmock::Method::{GET, POST, PUT};
    use httpmock::MockServer;

    #[test]
    fn inbound_message_decodes_provider_reply_target_without_referenced_message_text() {
        let message: DiscordTransportInboundMessage = serde_json::from_str(
            r#"{"id":"m-2","channel_id":"channel-1","content":"follow up","author":{"id":"user-1"},"message_reference":{"message_id":"m-1","referenced_message":{"content":"untrusted nested text"}}}"#,
        )
        .expect("parse Discord reply message");

        assert_eq!(message.reply_to_message_id(), Some("m-1"));
        let serialized = serde_json::to_value(&message).expect("serialize Discord message");
        assert_eq!(
            serialized["message_reference"],
            serde_json::json!({"message_id": "m-1"})
        );
    }

    #[test]
    fn inbound_message_without_reply_target_preserves_existing_serialization() {
        let payload = r#"{"id":"m-2","channel_id":"channel-1","content":"follow up","author":{"id":"user-1"}}"#;
        let message: DiscordTransportInboundMessage =
            serde_json::from_str(payload).expect("parse Discord message without reply");

        assert_eq!(message.reply_to_message_id(), None);
        let serialized = serde_json::to_value(&message).expect("serialize Discord message");
        assert!(serialized.get("message_reference").is_none());
    }

    #[test]
    fn inbound_message_normalizes_empty_reply_target_and_rejects_malformed_id() {
        let empty: DiscordTransportInboundMessage = serde_json::from_str(
            r#"{"id":"m-2","channel_id":"channel-1","author":{"id":"user-1"},"message_reference":{"message_id":"  "}}"#,
        )
        .expect("empty provider reply target is normalized away");
        assert_eq!(empty.reply_to_message_id(), None);

        let error = serde_json::from_str::<DiscordTransportInboundMessage>(
            r#"{"id":"m-2","channel_id":"channel-1","author":{"id":"user-1"},"message_reference":{"message_id":[]}}"#,
        )
        .expect_err("a non-string Discord message ID must be rejected");
        assert!(!error.to_string().is_empty());

        for invalid in [format!("m-{}", "x".repeat(129)), "m-\n1".to_string()] {
            let payload = serde_json::json!({
                "id": "m-2",
                "channel_id": "channel-1",
                "author": {"id": "user-1"},
                "message_reference": {"message_id": invalid},
            });
            assert!(serde_json::from_value::<DiscordTransportInboundMessage>(payload).is_err());
        }
    }

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

    #[test]
    fn send_message_retries_on_retryable_http_failure() {
        let server = MockServer::start();
        let retry_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/channels/channel-1/messages")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "1");
            then.status(429)
                .header("content-type", "application/json")
                .body(r#"{"message":"rate limited"}"#);
        });
        let success_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/channels/channel-1/messages")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "2");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"id":"msg-42","channel_id":"channel-1"}"#);
        });

        let client = DiscordTransportClient::new(DiscordTransportConfig {
            api_base_url: server.base_url(),
            bot_token: "discord-token".to_string(),
            timeout_ms: 1_000,
            retry_attempts: 2,
        })
        .expect("create transport client");
        let response = client
            .send_message_with_retry(&DiscordTransportOutboundRequest {
                channel_id: "channel-1".to_string(),
                content: "hello".to_string(),
                reply_to_message_id: None,
            })
            .expect("send message");
        assert_eq!(response.message_id, "msg-42");
        retry_mock.assert_hits(1);
        success_mock.assert_hits(1);
    }

    #[test]
    fn send_message_does_not_retry_non_retryable_failure() {
        let server = MockServer::start();
        let bad_request_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/channels/channel-1/messages")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "1");
            then.status(400)
                .header("content-type", "application/json")
                .body(r#"{"message":"bad request"}"#);
        });

        let client = DiscordTransportClient::new(DiscordTransportConfig {
            api_base_url: server.base_url(),
            bot_token: "discord-token".to_string(),
            timeout_ms: 1_000,
            retry_attempts: 3,
        })
        .expect("create transport client");
        let err = client
            .send_message_with_retry(&DiscordTransportOutboundRequest {
                channel_id: "channel-1".to_string(),
                content: "hello".to_string(),
                reply_to_message_id: None,
            })
            .expect_err("expected failure");
        assert!(err.to_string().contains("HTTP 400"));
        bad_request_mock.assert_hits(1);
    }

    #[test]
    fn send_message_supports_reply_reference_payload() {
        let server = MockServer::start();
        let reply_mock = server.mock(|when, then| {
            when.method(POST)
                .path("/channels/channel-1/messages")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "1")
                .body_contains("\"message_reference\"");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"id":"msg-reply","channel_id":"channel-1"}"#);
        });

        let client = DiscordTransportClient::new(DiscordTransportConfig {
            api_base_url: server.base_url(),
            bot_token: "discord-token".to_string(),
            timeout_ms: 1_000,
            retry_attempts: 1,
        })
        .expect("create transport client");
        let response = client
            .send_message_with_retry(&DiscordTransportOutboundRequest {
                channel_id: "channel-1".to_string(),
                content: "reply".to_string(),
                reply_to_message_id: Some("origin-1".to_string()),
            })
            .expect("send reply");
        assert_eq!(response.message_id, "msg-reply");
        reply_mock.assert_hits(1);
    }

    #[test]
    fn pin_message_retries_on_retryable_failure() {
        let server = MockServer::start();
        let retry_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/channels/channel-1/pins/message-1")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "1");
            then.status(500)
                .header("content-type", "application/json")
                .body(r#"{"message":"upstream error"}"#);
        });
        let success_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/channels/channel-1/pins/message-1")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "2");
            then.status(204);
        });
        let client = DiscordTransportClient::new(DiscordTransportConfig {
            api_base_url: server.base_url(),
            bot_token: "discord-token".to_string(),
            timeout_ms: 1_000,
            retry_attempts: 2,
        })
        .expect("create transport client");
        client
            .pin_message_with_retry(&DiscordPinMessageRequest {
                channel_id: "channel-1".to_string(),
                message_id: "message-1".to_string(),
            })
            .expect("pin message");
        retry_mock.assert_hits(1);
        success_mock.assert_hits(1);
    }

    #[test]
    fn add_reaction_encodes_emoji_path_and_succeeds() {
        let server = MockServer::start();
        let reaction_mock = server.mock(|when, then| {
            when.method(PUT)
                .path("/channels/channel-1/messages/message-1/reactions/%F0%9F%91%8D/@me")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "1");
            then.status(204);
        });
        let client = DiscordTransportClient::new(DiscordTransportConfig {
            api_base_url: server.base_url(),
            bot_token: "discord-token".to_string(),
            timeout_ms: 1_000,
            retry_attempts: 1,
        })
        .expect("create transport client");
        client
            .add_reaction_with_retry(&DiscordReactionRequest {
                channel_id: "channel-1".to_string(),
                message_id: "message-1".to_string(),
                emoji: "👍".to_string(),
            })
            .expect("add reaction");
        reaction_mock.assert_hits(1);
    }

    #[test]
    fn get_channel_messages_retries_and_parses_payload() {
        let server = MockServer::start();
        let retry_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/channels/channel-2/messages")
                .query_param("limit", "5")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "1");
            then.status(500)
                .header("content-type", "application/json")
                .body(r#"{"message":"upstream error"}"#);
        });
        let success_mock = server.mock(|when, then| {
            when.method(GET)
                .path("/channels/channel-2/messages")
                .query_param("limit", "5")
                .header(DISCORD_RETRY_ATTEMPT_HEADER, "2");
            then.status(200).header("content-type", "application/json").body(
                r#"[{"id":"m-1","channel_id":"channel-2","guild_id":"g-1","content":"hello","author":{"id":"u-1","bot":false},"mentions":[{"id":"b-1"}]}]"#,
            );
        });

        let client = DiscordTransportClient::new(DiscordTransportConfig {
            api_base_url: server.base_url(),
            bot_token: "discord-token".to_string(),
            timeout_ms: 1_000,
            retry_attempts: 2,
        })
        .expect("create transport client");
        let messages = client
            .get_channel_messages_with_retry("channel-2", 5)
            .expect("list channel messages");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "m-1");
        assert_eq!(messages[0].author.id, "u-1");
        assert_eq!(messages[0].mentions[0].id, "b-1");
        retry_mock.assert_hits(1);
        success_mock.assert_hits(1);
    }
}
