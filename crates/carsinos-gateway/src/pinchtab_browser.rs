use carsinos_protocol::RuntimePinchTabConfig;
use reqwest::{Client, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;
use url::Url;

pub const TOOL_BROWSER_HEALTH: &str = "assistant.browser.health";
pub const TOOL_BROWSER_NAVIGATE: &str = "assistant.browser.navigate";
pub const TOOL_BROWSER_TEXT: &str = "assistant.browser.text";
pub const TOOL_BROWSER_SNAPSHOT: &str = "assistant.browser.snapshot";
pub const TOOL_BROWSER_CAPTURE: &str = "assistant.browser.capture";

pub const BROWSER_TOOL_NAMES: &[&str] = &[
    TOOL_BROWSER_HEALTH,
    TOOL_BROWSER_NAVIGATE,
    TOOL_BROWSER_TEXT,
    TOOL_BROWSER_SNAPSHOT,
    TOOL_BROWSER_CAPTURE,
];

#[derive(Debug, Clone)]
pub struct PinchTabClient {
    http: Client,
    config: RuntimePinchTabConfig,
    server_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserAgentContext {
    pub agent_id: String,
    pub root_session_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrowserNavigateArgs {
    pub url: String,
    #[serde(default)]
    pub tab_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrowserReadArgs {
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PinchTabSessionCreateRequest<'a> {
    #[serde(rename = "agentId")]
    agent_id: &'a str,
    label: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
struct PinchTabSessionCreateResponse {
    #[serde(rename = "sessionToken")]
    session_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PinchTabNavigateRequest<'a> {
    url: &'a str,
}

#[derive(Debug, Clone, Serialize)]
struct PinchTabCaptureRequest {
    #[serde(rename = "requirePair")]
    require_pair: bool,
}

#[derive(Debug, Clone)]
pub struct PinchTabBrowserError {
    pub code: &'static str,
    pub message: String,
}

impl PinchTabBrowserError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl PinchTabClient {
    pub fn new(http: Client, config: RuntimePinchTabConfig, server_token: Option<String>) -> Self {
        Self {
            http,
            config,
            server_token: server_token
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        }
    }

    pub async fn health(&self) -> Result<Value, PinchTabBrowserError> {
        self.call(Method::GET, "/health", None, None, false).await
    }

    pub async fn navigate(
        &self,
        args: BrowserNavigateArgs,
        context: &BrowserAgentContext,
    ) -> Result<Value, PinchTabBrowserError> {
        ensure_url_allowed(&args.url, &self.config.allowed_domains)?;
        let path = match normalized_tab_id(args.tab_id.as_deref())? {
            Some(tab_id) => format!("/tabs/{tab_id}/navigate"),
            None => "/navigate".to_string(),
        };
        self.call(
            Method::POST,
            &path,
            Some(
                serde_json::to_value(PinchTabNavigateRequest { url: &args.url }).map_err(
                    |err| PinchTabBrowserError::new("BROWSER_INVALID_INPUT", err.to_string()),
                )?,
            ),
            Some(context),
            true,
        )
        .await
    }

    pub async fn text(
        &self,
        args: BrowserReadArgs,
        context: &BrowserAgentContext,
    ) -> Result<Value, PinchTabBrowserError> {
        let path = match normalized_tab_id(args.tab_id.as_deref())? {
            Some(tab_id) => format!("/tabs/{tab_id}/text"),
            None => "/text".to_string(),
        };
        self.call(Method::GET, &path, None, Some(context), true)
            .await
    }

    pub async fn snapshot(
        &self,
        args: BrowserReadArgs,
        context: &BrowserAgentContext,
    ) -> Result<Value, PinchTabBrowserError> {
        let mut path = match normalized_tab_id(args.tab_id.as_deref())? {
            Some(tab_id) => format!("/tabs/{tab_id}/snapshot"),
            None => "/snapshot".to_string(),
        };
        let mut query = Vec::new();
        if let Some(format) = normalized_optional_arg(args.format.as_deref()) {
            query.push(("format", format));
        }
        if let Some(filter) = normalized_optional_arg(args.filter.as_deref()) {
            query.push(("filter", filter));
        }
        if !query.is_empty() {
            let encoded = serde_urlencoded::to_string(&query).map_err(|err| {
                PinchTabBrowserError::new("BROWSER_INVALID_INPUT", err.to_string())
            })?;
            path.push('?');
            path.push_str(&encoded);
        }
        self.call(Method::GET, &path, None, Some(context), true)
            .await
    }

    pub async fn capture(
        &self,
        args: BrowserReadArgs,
        context: &BrowserAgentContext,
    ) -> Result<Value, PinchTabBrowserError> {
        let path = match normalized_tab_id(args.tab_id.as_deref())? {
            Some(tab_id) => format!("/tabs/{tab_id}/capture"),
            None => "/capture".to_string(),
        };
        self.call(
            Method::POST,
            &path,
            Some(
                serde_json::to_value(PinchTabCaptureRequest { require_pair: true }).map_err(
                    |err| PinchTabBrowserError::new("BROWSER_INVALID_INPUT", err.to_string()),
                )?,
            ),
            Some(context),
            true,
        )
        .await
    }

    async fn call(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        context: Option<&BrowserAgentContext>,
        requires_agent_session: bool,
    ) -> Result<Value, PinchTabBrowserError> {
        if !self.config.enabled {
            return Err(PinchTabBrowserError::new(
                "BROWSER_DISABLED",
                "PinchTab browser tools are disabled in runtime config",
            ));
        }

        let authorization = if requires_agent_session && self.config.use_agent_sessions {
            let context = context.ok_or_else(|| {
                PinchTabBrowserError::new(
                    "BROWSER_SESSION_REQUIRED",
                    "PinchTab agent session requires CarsinOS agent context",
                )
            })?;
            let session_token = self.create_session(context).await?;
            Some(format!("Session {session_token}"))
        } else {
            self.server_token
                .as_ref()
                .map(|token| format!("Bearer {token}"))
        };

        let url = self.endpoint(path)?;
        let mut request = self
            .http
            .request(method, url)
            .timeout(Duration::from_millis(self.config.timeout_ms.max(250)));
        if let Some(auth) = authorization {
            request = request.header(reqwest::header::AUTHORIZATION, auth);
        }
        if let Some(context) = context {
            request = request
                .header("X-OpenClaw-Agent-Id", &context.agent_id)
                .header("X-OpenClaw-Session-Id", &context.root_session_id);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        decode_pinchtab_response(request.send().await)
            .await
            .map(sanitize_browser_output)
    }

    async fn create_session(
        &self,
        context: &BrowserAgentContext,
    ) -> Result<String, PinchTabBrowserError> {
        let agent_id = context.agent_id.trim();
        if agent_id.is_empty() {
            return Err(PinchTabBrowserError::new(
                "BROWSER_SESSION_REQUIRED",
                "PinchTab session creation requires a non-empty CarsinOS agent id",
            ));
        }
        let token = self.server_token.as_ref().ok_or_else(|| {
            PinchTabBrowserError::new(
                "BROWSER_SESSION_REQUIRED",
                "PinchTab token_secret_ref is required when agent sessions are enabled",
            )
        })?;
        let label = format!("carsinos:{}", context.root_session_id);
        let request = self
            .http
            .post(self.endpoint("/sessions")?)
            .timeout(Duration::from_millis(self.config.timeout_ms.max(250)))
            .bearer_auth(token)
            .json(&PinchTabSessionCreateRequest {
                agent_id,
                label: &label,
            });
        let value = decode_pinchtab_response(request.send().await).await?;
        let parsed: PinchTabSessionCreateResponse =
            serde_json::from_value(value).map_err(|err| {
                PinchTabBrowserError::new(
                    "BROWSER_BAD_RESPONSE",
                    format!("PinchTab /sessions response is malformed: {err}"),
                )
            })?;
        parsed
            .session_token
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                PinchTabBrowserError::new(
                    "BROWSER_SESSION_REQUIRED",
                    "PinchTab /sessions response missing sessionToken",
                )
            })
    }

    fn endpoint(&self, path: &str) -> Result<Url, PinchTabBrowserError> {
        let mut base = self
            .config
            .base_url
            .trim()
            .trim_end_matches('/')
            .to_string();
        if base.is_empty() {
            base = "http://127.0.0.1:9867".to_string();
        }
        let base = Url::parse(&(base + "/")).map_err(|err| {
            PinchTabBrowserError::new("BROWSER_INVALID_CONFIG", format!("invalid base_url: {err}"))
        })?;
        base.join(path.trim_start_matches('/')).map_err(|err| {
            PinchTabBrowserError::new("BROWSER_INVALID_INPUT", format!("invalid path: {err}"))
        })
    }
}

async fn decode_pinchtab_response(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<Value, PinchTabBrowserError> {
    let response = response.map_err(|err| {
        if err.is_timeout() {
            PinchTabBrowserError::new("BROWSER_TIMEOUT", "PinchTab request timed out")
        } else {
            PinchTabBrowserError::new(
                "BROWSER_UNAVAILABLE",
                format!("PinchTab request failed: {err}"),
            )
        }
    })?;
    let status = response.status();
    let text = response.text().await.map_err(|err| {
        PinchTabBrowserError::new(
            "BROWSER_BAD_RESPONSE",
            format!("PinchTab response body could not be read: {err}"),
        )
    })?;
    if !status.is_success() {
        return Err(PinchTabBrowserError::new(
            http_status_code(status),
            format!(
                "PinchTab returned HTTP {}: {}",
                status.as_u16(),
                truncate_for_message(&text)
            ),
        ));
    }
    if text.trim().is_empty() {
        return Ok(serde_json::json!({ "status": "ok" }));
    }
    serde_json::from_str::<Value>(&text).or_else(|_| Ok(serde_json::json!({ "text": text })))
}

fn http_status_code(status: StatusCode) -> &'static str {
    match status {
        StatusCode::UNAUTHORIZED => "BROWSER_UNAUTHORIZED",
        StatusCode::FORBIDDEN => "BROWSER_FORBIDDEN",
        StatusCode::NOT_FOUND => "BROWSER_NOT_FOUND",
        _ => "BROWSER_UPSTREAM_ERROR",
    }
}

pub fn ensure_url_allowed(
    raw_url: &str,
    allowed_domains: &[String],
) -> Result<(), PinchTabBrowserError> {
    let trimmed = raw_url.trim();
    if trimmed.eq_ignore_ascii_case("about:blank") {
        return Ok(());
    }
    let parsed = Url::parse(trimmed).map_err(|err| {
        PinchTabBrowserError::new("BROWSER_INVALID_INPUT", format!("invalid url: {err}"))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(PinchTabBrowserError::new(
            "BROWSER_POLICY_DENY",
            "browser navigation only allows http, https, or about:blank URLs",
        ));
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() {
        return Err(PinchTabBrowserError::new(
            "BROWSER_POLICY_DENY",
            "browser navigation URL must include a host",
        ));
    }
    if allowed_domains
        .iter()
        .any(|allowed| domain_matches(&host, allowed))
    {
        return Ok(());
    }
    Err(PinchTabBrowserError::new(
        "BROWSER_POLICY_DENY",
        format!("host {host} is not in extensions.browser.pinchtab.allowed_domains"),
    ))
}

fn domain_matches(host: &str, allowed: &str) -> bool {
    let allowed = allowed.trim().trim_end_matches('.').to_ascii_lowercase();
    if allowed.is_empty() {
        return false;
    }
    if let Some(suffix) = allowed.strip_prefix("*.") {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    if let Some(suffix) = allowed.strip_prefix('.') {
        return host == suffix || host.ends_with(&format!(".{suffix}"));
    }
    host == allowed
}

pub fn sanitize_browser_output(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    let lowered = key.to_ascii_lowercase();
                    if matches!(
                        lowered.as_str(),
                        "authorization"
                            | "cookie"
                            | "cookies"
                            | "set-cookie"
                            | "proxy-authorization"
                            | "sessiontoken"
                            | "token"
                            | "access_token"
                            | "refresh_token"
                            | "id_token"
                            | "api_key"
                            | "x-api-key"
                    ) {
                        (key, Value::String("[redacted]".to_string()))
                    } else {
                        (key, sanitize_browser_output(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(sanitize_browser_output)
                .collect::<Vec<_>>(),
        ),
        other => other,
    }
}

fn normalized_tab_id(value: Option<&str>) -> Result<Option<String>, PinchTabBrowserError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('?')
        || value.contains('#')
        || value.contains("..")
    {
        return Err(PinchTabBrowserError::new(
            "BROWSER_INVALID_INPUT",
            "PinchTab tab_id must be a safe path segment",
        ));
    }
    Ok(Some(value.to_string()))
}

fn normalized_optional_arg(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn truncate_for_message(text: &str) -> String {
    const MAX: usize = 500;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    let prefix = text.chars().take(MAX).collect::<String>();
    format!("{prefix}...[truncated]")
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::POST;
    use httpmock::MockServer;

    fn enabled_config(base_url: String) -> RuntimePinchTabConfig {
        RuntimePinchTabConfig {
            enabled: true,
            base_url,
            token_secret_ref: Some("runtime.browser.pinchtab.token".to_string()),
            allowed_domains: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "*.example.test".to_string(),
            ],
            timeout_ms: 5_000,
            use_agent_sessions: true,
            default_profile: None,
            risk_gates: Default::default(),
        }
    }

    #[test]
    fn navigation_policy_allows_exact_and_wildcard_hosts() {
        let allowed = vec!["localhost".to_string(), "*.example.test".to_string()];
        assert!(ensure_url_allowed("http://localhost:3000", &allowed).is_ok());
        assert!(ensure_url_allowed("https://app.example.test", &allowed).is_ok());
        assert!(ensure_url_allowed("https://example.test", &allowed).is_ok());
        assert!(ensure_url_allowed("about:blank", &allowed).is_ok());
    }

    #[test]
    fn navigation_policy_blocks_unlisted_hosts_and_non_http_schemes() {
        let allowed = vec!["example.test".to_string()];
        assert_eq!(
            ensure_url_allowed("https://evil.test", &allowed)
                .expect_err("policy deny")
                .code,
            "BROWSER_POLICY_DENY"
        );
        assert_eq!(
            ensure_url_allowed("file:///etc/passwd", &allowed)
                .expect_err("scheme deny")
                .code,
            "BROWSER_POLICY_DENY"
        );
    }

    #[test]
    fn browser_output_redacts_tokens_and_cookies_recursively() {
        let value = serde_json::json!({
            "Authorization": "Bearer abc",
            "nested": {"sessionToken": "secret", "ok": true},
            "headers": [{"cookie": "a=b", "x-api-key": "secret", "proxy-authorization": "secret"}]
        });
        let redacted = sanitize_browser_output(value);
        assert_eq!(redacted["Authorization"], "[redacted]");
        assert_eq!(redacted["nested"]["sessionToken"], "[redacted]");
        assert_eq!(redacted["headers"][0]["cookie"], "[redacted]");
        assert_eq!(redacted["headers"][0]["x-api-key"], "[redacted]");
        assert_eq!(redacted["headers"][0]["proxy-authorization"], "[redacted]");
        assert_eq!(redacted["nested"]["ok"], true);
    }

    #[test]
    fn tab_id_rejects_unsafe_path_segments() {
        for value in ["../secret", "a/b", "a\\b", "tab?x=1", "tab#frag"] {
            let err = normalized_tab_id(Some(value)).expect_err("unsafe tab id");
            assert_eq!(err.code, "BROWSER_INVALID_INPUT");
        }
        assert_eq!(
            normalized_tab_id(Some(" tab-1 "))
                .expect("safe tab id")
                .as_deref(),
            Some("tab-1")
        );
    }

    #[test]
    fn truncate_for_message_handles_multibyte_boundaries() {
        let text = "é".repeat(600);
        let truncated = truncate_for_message(&text);
        assert!(truncated.ends_with("...[truncated]"));
        assert_eq!(
            truncated.trim_end_matches("...[truncated]").chars().count(),
            500
        );
    }

    #[tokio::test]
    async fn navigate_creates_agent_session_and_uses_session_token() {
        let server = MockServer::start_async().await;
        let session_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/sessions")
                    .header("authorization", "Bearer server-token")
                    .json_body(serde_json::json!({
                        "agentId": "agent-1",
                        "label": "carsinos:session-1"
                    }));
                then.status(200).json_body(serde_json::json!({
                    "sessionToken": "agent-session-token"
                }));
            })
            .await;
        let navigate_mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/navigate")
                    .header("authorization", "Session agent-session-token")
                    .header("x-openclaw-agent-id", "agent-1")
                    .header("x-openclaw-session-id", "session-1")
                    .json_body(serde_json::json!({"url": "https://app.example.test"}));
                then.status(200).json_body(serde_json::json!({
                    "tabId": "tab_1",
                    "url": "https://app.example.test"
                }));
            })
            .await;
        let client = PinchTabClient::new(
            Client::new(),
            enabled_config(server.base_url()),
            Some("server-token".to_string()),
        );

        let result = client
            .navigate(
                BrowserNavigateArgs {
                    url: "https://app.example.test".to_string(),
                    tab_id: None,
                },
                &BrowserAgentContext {
                    agent_id: "agent-1".to_string(),
                    root_session_id: "session-1".to_string(),
                },
            )
            .await
            .expect("navigate");

        assert_eq!(result["tabId"], "tab_1");
        assert_eq!(session_mock.hits_async().await, 1);
        assert_eq!(navigate_mock.hits_async().await, 1);
    }

    #[tokio::test]
    async fn agent_session_creation_fails_closed_without_server_token() {
        let server = MockServer::start_async().await;
        let client = PinchTabClient::new(Client::new(), enabled_config(server.base_url()), None);
        let err = client
            .text(
                BrowserReadArgs::default(),
                &BrowserAgentContext {
                    agent_id: "agent-1".to_string(),
                    root_session_id: "session-1".to_string(),
                },
            )
            .await
            .expect_err("missing token");
        assert_eq!(err.code, "BROWSER_SESSION_REQUIRED");
    }
}
