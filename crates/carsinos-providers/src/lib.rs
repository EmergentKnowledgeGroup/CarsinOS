use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct ProviderAuthProfile {
    pub auth_profile_id: Option<String>,
    pub auth_mode: String,
    pub risk_level: String,
    pub api_base_url: Option<String>,
    pub credentials_json: String,
}

impl ProviderAuthProfile {
    fn api_key(&self) -> Result<String> {
        let payload: serde_json::Value = serde_json::from_str(&self.credentials_json)
            .context("failed to parse provider credentials_json")?;
        for key in ["api_key", "token", "access_token", "bearer_token"] {
            if let Some(value) = payload.get(key).and_then(|value| value.as_str()) {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Ok(trimmed.to_string());
                }
            }
        }
        anyhow::bail!("provider credentials missing API token material")
    }
}

#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub model_provider: String,
    pub model_id: String,
    pub input: String,
    pub auth_profile: Option<ProviderAuthProfile>,
}

#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub output_text: String,
    pub deltas: Vec<String>,
}

#[async_trait]
pub trait ProviderClient: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse>;
}

#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    http_client: Client,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .expect("provider HTTP client initialization failed");
        Self { http_client }
    }

    pub async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        match request.model_provider.as_str() {
            "mock" | "unconfigured" => EchoProvider.complete(request).await,
            "openai" => complete_openai(&self.http_client, request).await,
            "anthropic" => complete_anthropic(&self.http_client, request).await,
            provider => anyhow::bail!("unsupported model provider: {provider}"),
        }
    }
}

struct EchoProvider;

#[async_trait]
impl ProviderClient for EchoProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse> {
        let trimmed = request.input.trim();
        let output_text = if trimmed.is_empty() {
            format!(
                "Model {}:{} received no user input in the current session.",
                request.model_provider, request.model_id
            )
        } else {
            format!(
                "Model {}:{} response: {}",
                request.model_provider, request.model_id, trimmed
            )
        };

        Ok(CompletionResponse {
            deltas: split_word_deltas(&output_text),
            output_text,
        })
    }
}

async fn complete_openai(
    client: &Client,
    request: CompletionRequest,
) -> Result<CompletionResponse> {
    let auth = require_auth_profile("openai", &request)?;
    let token = auth.api_key().map_err(|err| {
        anyhow!(
            "PROVIDER_ERROR:openai:AUTH_REQUIRED:invalid_credentials:{}",
            err.to_string().replace(':', "_")
        )
    })?;

    let base_url = auth
        .api_base_url
        .as_deref()
        .unwrap_or("https://api.openai.com")
        .trim_end_matches('/');
    let url = format!("{base_url}/v1/chat/completions");

    let body = json!({
        "model": request.model_id,
        "messages": [
            {
                "role": "user",
                "content": request.input
            }
        ],
        "stream": false
    });

    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("openai completion request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(normalize_provider_http_error("openai", status, &body));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .context("failed to parse openai completion response JSON")?;
    let output_text = extract_openai_content(&payload)
        .ok_or_else(|| anyhow!("PROVIDER_ERROR:openai:INTERNAL_ERROR:missing_output_content"))?;

    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
    })
}

async fn complete_anthropic(
    client: &Client,
    request: CompletionRequest,
) -> Result<CompletionResponse> {
    let auth = require_auth_profile("anthropic", &request)?;
    let token = auth.api_key().map_err(|err| {
        anyhow!(
            "PROVIDER_ERROR:anthropic:AUTH_REQUIRED:invalid_credentials:{}",
            err.to_string().replace(':', "_")
        )
    })?;

    let base_url = auth
        .api_base_url
        .as_deref()
        .unwrap_or("https://api.anthropic.com")
        .trim_end_matches('/');
    let url = format!("{base_url}/v1/messages");

    let body = json!({
        "model": request.model_id,
        "max_tokens": 1024,
        "messages": [
            {
                "role": "user",
                "content": request.input
            }
        ]
    });

    let response = client
        .post(url)
        .header("x-api-key", token)
        .header("anthropic-version", "2023-06-01")
        .json(&body)
        .send()
        .await
        .context("anthropic completion request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(normalize_provider_http_error("anthropic", status, &body));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .context("failed to parse anthropic completion response JSON")?;
    let output_text = extract_anthropic_content(&payload)
        .ok_or_else(|| anyhow!("PROVIDER_ERROR:anthropic:INTERNAL_ERROR:missing_output_content"))?;

    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
    })
}

fn require_auth_profile<'a>(
    provider: &str,
    request: &'a CompletionRequest,
) -> Result<&'a ProviderAuthProfile> {
    request
        .auth_profile
        .as_ref()
        .ok_or_else(|| anyhow!("PROVIDER_ERROR:{provider}:AUTH_REQUIRED:missing_auth_profile"))
}

fn normalize_provider_http_error(provider: &str, status: StatusCode, body: &str) -> anyhow::Error {
    let code = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => "AUTH_REQUIRED",
        StatusCode::TOO_MANY_REQUESTS => "RATE_LIMITED",
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => "TIMEOUT",
        _ if status.is_server_error() => "DEPENDENCY_UNAVAILABLE",
        _ => "INTERNAL_ERROR",
    };

    let body = body.trim();
    let body = if body.len() > 300 { &body[..300] } else { body };
    anyhow!(
        "PROVIDER_ERROR:{provider}:{code}:status={}:body={}",
        status.as_u16(),
        body
    )
}

fn split_word_deltas(output_text: &str) -> Vec<String> {
    let mut deltas = Vec::new();
    for (idx, word) in output_text.split_whitespace().enumerate() {
        if idx == 0 {
            deltas.push(word.to_string());
        } else {
            deltas.push(format!(" {word}"));
        }
    }
    deltas
}

fn extract_openai_content(payload: &serde_json::Value) -> Option<String> {
    let content = payload
        .get("choices")
        .and_then(|choices| choices.as_array())?
        .first()?
        .get("message")?
        .get("content")?;

    if let Some(value) = content.as_str() {
        return Some(value.to_string());
    }

    // Handle array-form content payloads used by some compatible providers.
    let content_items = content.as_array()?;
    let mut combined = String::new();
    for item in content_items {
        if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
            combined.push_str(text);
        }
    }

    if combined.trim().is_empty() {
        None
    } else {
        Some(combined)
    }
}

fn extract_anthropic_content(payload: &serde_json::Value) -> Option<String> {
    let content_items = payload.get("content")?.as_array()?;
    let mut combined = String::new();
    for item in content_items {
        if item.get("type").and_then(|value| value.as_str()) == Some("text") {
            if let Some(text) = item.get("text").and_then(|value| value.as_str()) {
                combined.push_str(text);
            }
        }
    }

    if combined.trim().is_empty() {
        None
    } else {
        Some(combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::POST;
    use httpmock::MockServer;

    #[tokio::test]
    async fn openai_provider_returns_output_with_auth_profile() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "message": {
                                "content": "openai says hi"
                            }
                        }
                    ]
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "openai".to_string(),
                model_id: "gpt-test".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p1".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: Some(server.base_url()),
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            })
            .await
            .expect("openai completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "openai says hi");
    }

    #[tokio::test]
    async fn anthropic_provider_returns_output_with_auth_profile() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/messages");
                then.status(200).json_body(json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "anthropic says hi"
                        }
                    ]
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "anthropic".to_string(),
                model_id: "claude-test".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p2".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: Some(server.base_url()),
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            })
            .await
            .expect("anthropic completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "anthropic says hi");
    }

    #[tokio::test]
    async fn provider_auth_is_required_for_external_providers() {
        let registry = ProviderRegistry::new();
        let error = registry
            .complete(CompletionRequest {
                model_provider: "openai".to_string(),
                model_id: "gpt-test".to_string(),
                input: "hello".to_string(),
                auth_profile: None,
            })
            .await
            .expect_err("expected missing auth error");

        assert!(error
            .to_string()
            .contains("PROVIDER_ERROR:openai:AUTH_REQUIRED"));
    }
}
