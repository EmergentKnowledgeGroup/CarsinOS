use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Stdio;
use std::time::Duration;

const AGENT_SDK_AUTH_MODE: &str = "agent_sdk";
const HEADLESS_DEFAULT_COMMAND: &str = "claude";
const HEADLESS_DEFAULT_TIMEOUT_MS: u64 = 45_000;
const PROVIDER_VLLM_API_BASE_URL_ENV: &str = "CARSINOS_PROVIDER_VLLM_API_BASE_URL";
const PROVIDER_OLLAMA_API_BASE_URL_ENV: &str = "CARSINOS_PROVIDER_OLLAMA_API_BASE_URL";
const PROVIDER_VLLM_DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:8000";
const PROVIDER_OLLAMA_DEFAULT_API_BASE_URL: &str = "http://127.0.0.1:11434";

#[derive(Debug, Clone)]
pub struct ProviderAuthProfile {
    pub auth_profile_id: Option<String>,
    pub auth_mode: String,
    pub risk_level: String,
    pub api_base_url: Option<String>,
    pub credentials_json: String,
}

impl ProviderAuthProfile {
    fn credentials_payload(&self) -> Result<serde_json::Value> {
        serde_json::from_str(&self.credentials_json)
            .context("failed to parse provider credentials_json")
    }

    fn api_key(&self) -> Result<String> {
        let payload = self.credentials_payload()?;
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
    pub usage: CompletionUsageMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionUsageMetrics {
    pub input_chars: u64,
    pub output_chars: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProviderErrorClass {
    AuthRequired,
    RateLimited,
    Timeout,
    DependencyUnavailable,
    InternalError,
}

impl ProviderErrorClass {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::RateLimited => "RATE_LIMITED",
            Self::Timeout => "TIMEOUT",
            Self::DependencyUnavailable => "DEPENDENCY_UNAVAILABLE",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::AuthRequired | Self::RateLimited | Self::Timeout | Self::DependencyUnavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub provider: String,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_json_mode: bool,
    pub supports_vision: bool,
    pub max_context_tokens: Option<u32>,
    pub error_classes: Vec<String>,
    pub retryable_error_classes: Vec<String>,
}

pub fn parse_provider_error_class(error: &str) -> ProviderErrorClass {
    if let Some(rest) = error.strip_prefix("PROVIDER_ERROR:") {
        let mut parts = rest.split(':');
        let _provider = parts.next();
        if let Some(code) = parts.next() {
            return match code {
                "AUTH_REQUIRED" => ProviderErrorClass::AuthRequired,
                "RATE_LIMITED" => ProviderErrorClass::RateLimited,
                "TIMEOUT" => ProviderErrorClass::Timeout,
                "DEPENDENCY_UNAVAILABLE" => ProviderErrorClass::DependencyUnavailable,
                _ => ProviderErrorClass::InternalError,
            };
        }
    }
    ProviderErrorClass::InternalError
}

pub fn provider_error_retryable(error: ProviderErrorClass) -> bool {
    error.retryable()
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
            "openrouter" => complete_openrouter(&self.http_client, request).await,
            "ollama" => complete_ollama(&self.http_client, request).await,
            "vllm" => complete_vllm(&self.http_client, request).await,
            "lmstudio" => complete_lmstudio(&self.http_client, request).await,
            provider => anyhow::bail!("unsupported model provider: {provider}"),
        }
    }

    pub fn list_capabilities(&self) -> Vec<ProviderCapabilities> {
        [
            "mock",
            "unconfigured",
            "openai",
            "anthropic",
            "openrouter",
            "ollama",
            "vllm",
            "lmstudio",
        ]
        .into_iter()
        .filter_map(provider_capabilities)
        .collect()
    }

    pub fn capabilities(&self, provider: &str) -> Option<ProviderCapabilities> {
        provider_capabilities(provider)
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
        let usage = usage_from_token_counts(&request.input, &output_text, None, None, None, None);

        Ok(CompletionResponse {
            deltas: split_word_deltas(&output_text),
            output_text,
            usage,
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
    let usage = usage_from_openai_payload(&payload, &request.input, &output_text);

    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
        usage,
    })
}

async fn complete_anthropic(
    client: &Client,
    request: CompletionRequest,
) -> Result<CompletionResponse> {
    let auth = require_auth_profile("anthropic", &request)?;
    if auth
        .auth_mode
        .trim()
        .eq_ignore_ascii_case(AGENT_SDK_AUTH_MODE)
    {
        return complete_anthropic_headless(&request, auth).await;
    }
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
    let usage = usage_from_anthropic_payload(&payload, &request.input, &output_text);

    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
        usage,
    })
}

async fn complete_anthropic_headless(
    request: &CompletionRequest,
    auth: &ProviderAuthProfile,
) -> Result<CompletionResponse> {
    let credentials = auth.credentials_payload().map_err(|err| {
        anyhow!(
            "PROVIDER_ERROR:anthropic:AUTH_REQUIRED:invalid_credentials:{}",
            err.to_string().replace(':', "_")
        )
    })?;

    let command = credentials
        .get("headless_command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(HEADLESS_DEFAULT_COMMAND)
        .to_string();
    let mut args = credentials
        .get("headless_args")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str().map(str::trim))
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                "-p".to_string(),
                "{prompt}".to_string(),
                "--output-format".to_string(),
                "text".to_string(),
            ]
        });
    let timeout_ms = parse_u64_value(credentials.get("headless_timeout_ms"))
        .unwrap_or(HEADLESS_DEFAULT_TIMEOUT_MS)
        .clamp(1_000, 300_000);
    let workdir = credentials
        .get("headless_workdir")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string());

    let mut prompt_injected = false;
    let mut has_prompt_flag = false;
    for arg in &mut args {
        if arg == "-p" || arg == "--prompt" {
            has_prompt_flag = true;
        }
        if arg.contains("{prompt}") {
            *arg = arg.replace("{prompt}", request.input.as_str());
            prompt_injected = true;
        }
        if arg.contains("{model}") {
            *arg = arg.replace("{model}", request.model_id.as_str());
        }
    }
    if !prompt_injected {
        if !has_prompt_flag {
            args.push("-p".to_string());
        }
        args.push(request.input.clone());
    }

    let mut process = tokio::process::Command::new(command.as_str());
    process.args(args.iter());
    process.stdin(Stdio::null());
    process.stdout(Stdio::piped());
    process.stderr(Stdio::piped());
    process.kill_on_drop(true);
    if let Some(workdir) = workdir {
        process.current_dir(workdir);
    }
    let child = process.spawn().map_err(|err| {
        anyhow!(
            "PROVIDER_ERROR:anthropic:DEPENDENCY_UNAVAILABLE:headless_spawn_failed:{}",
            err.to_string().replace(':', "_")
        )
    })?;
    let output =
        match tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait_with_output())
            .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(err)) => {
                return Err(anyhow!(
                    "PROVIDER_ERROR:anthropic:DEPENDENCY_UNAVAILABLE:headless_spawn_failed:{}",
                    err.to_string().replace(':', "_")
                ));
            }
            Err(_) => {
                return Err(anyhow!(
                    "PROVIDER_ERROR:anthropic:TIMEOUT:headless_timeout_after_{}ms",
                    timeout_ms
                ));
            }
        };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        let stderr = stderr.chars().take(200).collect::<String>();
        let status = output.status.code().unwrap_or(-1);
        return Err(anyhow!(
            "PROVIDER_ERROR:anthropic:DEPENDENCY_UNAVAILABLE:headless_exit_status_{}:{}",
            status,
            stderr.replace(':', "_")
        ));
    }

    let output_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output_text.is_empty() {
        return Err(anyhow!(
            "PROVIDER_ERROR:anthropic:INTERNAL_ERROR:headless_output_empty"
        ));
    }

    let usage = usage_from_token_counts(&request.input, &output_text, None, None, None, None);
    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
        usage,
    })
}

async fn complete_openrouter(
    client: &Client,
    request: CompletionRequest,
) -> Result<CompletionResponse> {
    let auth = require_auth_profile("openrouter", &request)?;
    let token = auth.api_key().map_err(|err| {
        anyhow!(
            "PROVIDER_ERROR:openrouter:AUTH_REQUIRED:invalid_credentials:{}",
            err.to_string().replace(':', "_")
        )
    })?;
    let base_url = auth
        .api_base_url
        .as_deref()
        .unwrap_or("https://openrouter.ai/api")
        .trim_end_matches('/')
        .to_string();
    complete_openai_compatible(
        client,
        "openrouter",
        &base_url,
        &request.model_id,
        &request.input,
        Some(token),
    )
    .await
}

async fn complete_vllm(client: &Client, request: CompletionRequest) -> Result<CompletionResponse> {
    let (base_url, bearer_token) = if let Some(auth) = request.auth_profile.as_ref() {
        let token = auth.api_key().map_err(|err| {
            anyhow!(
                "PROVIDER_ERROR:vllm:AUTH_REQUIRED:invalid_credentials:{}",
                err.to_string().replace(':', "_")
            )
        })?;
        (
            provider_api_base_url(
                Some(auth),
                PROVIDER_VLLM_API_BASE_URL_ENV,
                PROVIDER_VLLM_DEFAULT_API_BASE_URL,
            ),
            Some(token),
        )
    } else {
        (
            provider_api_base_url(
                None,
                PROVIDER_VLLM_API_BASE_URL_ENV,
                PROVIDER_VLLM_DEFAULT_API_BASE_URL,
            ),
            None,
        )
    };
    complete_openai_compatible(
        client,
        "vllm",
        &base_url,
        &request.model_id,
        &request.input,
        bearer_token,
    )
    .await
}

async fn complete_lmstudio(
    client: &Client,
    request: CompletionRequest,
) -> Result<CompletionResponse> {
    let (base_url, bearer_token) = if let Some(auth) = request.auth_profile.as_ref() {
        let token = auth.api_key().map_err(|err| {
            anyhow!(
                "PROVIDER_ERROR:lmstudio:AUTH_REQUIRED:invalid_credentials:{}",
                err.to_string().replace(':', "_")
            )
        })?;
        (
            auth.api_base_url
                .as_deref()
                .unwrap_or("http://127.0.0.1:1234")
                .trim_end_matches('/')
                .to_string(),
            Some(token),
        )
    } else {
        ("http://127.0.0.1:1234".to_string(), None)
    };
    complete_openai_compatible(
        client,
        "lmstudio",
        &base_url,
        &request.model_id,
        &request.input,
        bearer_token,
    )
    .await
}

async fn complete_ollama(
    client: &Client,
    request: CompletionRequest,
) -> Result<CompletionResponse> {
    let (base_url, bearer_token) = if let Some(auth) = request.auth_profile.as_ref() {
        let token = auth.api_key().map_err(|err| {
            anyhow!(
                "PROVIDER_ERROR:ollama:AUTH_REQUIRED:invalid_credentials:{}",
                err.to_string().replace(':', "_")
            )
        })?;
        (
            provider_api_base_url(
                Some(auth),
                PROVIDER_OLLAMA_API_BASE_URL_ENV,
                PROVIDER_OLLAMA_DEFAULT_API_BASE_URL,
            ),
            Some(token),
        )
    } else {
        (
            provider_api_base_url(
                None,
                PROVIDER_OLLAMA_API_BASE_URL_ENV,
                PROVIDER_OLLAMA_DEFAULT_API_BASE_URL,
            ),
            None,
        )
    };
    let url = format!("{base_url}/api/chat");
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
    let mut req = client.post(url).json(&body);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let response = req
        .send()
        .await
        .context("ollama completion request failed")?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(normalize_provider_http_error("ollama", status, &body));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .context("failed to parse ollama completion response JSON")?;
    let output_text = extract_ollama_content(&payload)
        .ok_or_else(|| anyhow!("PROVIDER_ERROR:ollama:INTERNAL_ERROR:missing_output_content"))?;
    let usage = usage_from_ollama_payload(&payload, &request.input, &output_text);

    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
        usage,
    })
}

async fn complete_openai_compatible(
    client: &Client,
    provider: &str,
    base_url: &str,
    model_id: &str,
    input: &str,
    bearer_token: Option<String>,
) -> Result<CompletionResponse> {
    let url = format!("{base_url}/v1/chat/completions");
    let body = json!({
        "model": model_id,
        "messages": [
            {
                "role": "user",
                "content": input
            }
        ],
        "stream": false
    });
    let mut req = client.post(url).json(&body);
    if let Some(token) = bearer_token {
        req = req.bearer_auth(token);
    }
    let response = req
        .send()
        .await
        .with_context(|| format!("{provider} openai-compatible completion request failed"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(normalize_provider_http_error(provider, status, &body));
    }

    let payload: serde_json::Value = response
        .json()
        .await
        .with_context(|| format!("failed to parse {provider} completion response JSON"))?;
    let output_text = extract_openai_content(&payload).ok_or_else(|| {
        anyhow!("PROVIDER_ERROR:{provider}:INTERNAL_ERROR:missing_output_content")
    })?;
    let usage = usage_from_openai_payload(&payload, input, &output_text);

    Ok(CompletionResponse {
        deltas: split_word_deltas(&output_text),
        output_text,
        usage,
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
    let code = provider_error_class_from_status(status).as_code();

    let body = body.trim();
    let body = if body.len() > 300 { &body[..300] } else { body };
    anyhow!(
        "PROVIDER_ERROR:{provider}:{code}:status={}:body={}",
        status.as_u16(),
        body
    )
}

fn provider_api_base_url(
    auth_profile: Option<&ProviderAuthProfile>,
    env_var: &str,
    default: &str,
) -> String {
    auth_profile
        .and_then(|auth| auth.api_base_url.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var(env_var)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| default.to_string())
        .trim_end_matches('/')
        .to_string()
}

fn provider_error_class_from_status(status: StatusCode) -> ProviderErrorClass {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ProviderErrorClass::AuthRequired,
        StatusCode::TOO_MANY_REQUESTS => ProviderErrorClass::RateLimited,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => ProviderErrorClass::Timeout,
        _ if status.is_server_error() => ProviderErrorClass::DependencyUnavailable,
        _ => ProviderErrorClass::InternalError,
    }
}

fn provider_capabilities(provider: &str) -> Option<ProviderCapabilities> {
    let all_error_classes = vec![
        ProviderErrorClass::AuthRequired,
        ProviderErrorClass::RateLimited,
        ProviderErrorClass::Timeout,
        ProviderErrorClass::DependencyUnavailable,
        ProviderErrorClass::InternalError,
    ]
    .into_iter()
    .map(|class| class.as_code().to_string())
    .collect::<Vec<_>>();
    let retryable_error_classes = vec![
        ProviderErrorClass::AuthRequired,
        ProviderErrorClass::RateLimited,
        ProviderErrorClass::Timeout,
        ProviderErrorClass::DependencyUnavailable,
    ]
    .into_iter()
    .map(|class| class.as_code().to_string())
    .collect::<Vec<_>>();

    match provider {
        "mock" => Some(ProviderCapabilities {
            provider: "mock".to_string(),
            supports_streaming: true,
            supports_tools: false,
            supports_json_mode: false,
            supports_vision: false,
            max_context_tokens: Some(8_192),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "unconfigured" => Some(ProviderCapabilities {
            provider: "unconfigured".to_string(),
            supports_streaming: true,
            supports_tools: false,
            supports_json_mode: false,
            supports_vision: false,
            max_context_tokens: Some(8_192),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "openai" => Some(ProviderCapabilities {
            provider: "openai".to_string(),
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            max_context_tokens: Some(128_000),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "anthropic" => Some(ProviderCapabilities {
            provider: "anthropic".to_string(),
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            max_context_tokens: Some(200_000),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "openrouter" => Some(ProviderCapabilities {
            provider: "openrouter".to_string(),
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: true,
            max_context_tokens: Some(200_000),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "ollama" => Some(ProviderCapabilities {
            provider: "ollama".to_string(),
            supports_streaming: true,
            supports_tools: false,
            supports_json_mode: true,
            supports_vision: false,
            max_context_tokens: Some(32_000),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "vllm" => Some(ProviderCapabilities {
            provider: "vllm".to_string(),
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: false,
            max_context_tokens: Some(32_000),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        "lmstudio" => Some(ProviderCapabilities {
            provider: "lmstudio".to_string(),
            supports_streaming: true,
            supports_tools: true,
            supports_json_mode: true,
            supports_vision: false,
            max_context_tokens: Some(32_000),
            error_classes: all_error_classes,
            retryable_error_classes,
        }),
        _ => None,
    }
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

fn parse_u64_value(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(|item| {
        item.as_u64()
            .or_else(|| item.as_i64().and_then(|raw| u64::try_from(raw).ok()))
            .or_else(|| item.as_str().and_then(|raw| raw.trim().parse::<u64>().ok()))
    })
}

fn estimate_tokens_from_chars(chars: u64) -> u64 {
    ((chars.saturating_add(3)) / 4).max(1)
}

fn usage_from_token_counts(
    input: &str,
    output: &str,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
    estimated_cost_usd: Option<f64>,
) -> CompletionUsageMetrics {
    let input_chars = input.chars().count() as u64;
    let output_chars = output.chars().count() as u64;
    let resolved_input_tokens =
        input_tokens.unwrap_or_else(|| estimate_tokens_from_chars(input_chars));
    let resolved_output_tokens =
        output_tokens.unwrap_or_else(|| estimate_tokens_from_chars(output_chars));
    let resolved_total_tokens = total_tokens
        .unwrap_or_else(|| resolved_input_tokens.saturating_add(resolved_output_tokens));
    CompletionUsageMetrics {
        input_chars,
        output_chars,
        input_tokens: resolved_input_tokens,
        output_tokens: resolved_output_tokens,
        total_tokens: resolved_total_tokens,
        estimated_cost_usd,
    }
}

fn usage_from_openai_payload(
    payload: &serde_json::Value,
    input: &str,
    output: &str,
) -> CompletionUsageMetrics {
    let usage = payload.get("usage");
    let prompt_tokens = parse_u64_value(usage.and_then(|row| row.get("prompt_tokens")));
    let completion_tokens = parse_u64_value(usage.and_then(|row| row.get("completion_tokens")));
    let total_tokens = parse_u64_value(usage.and_then(|row| row.get("total_tokens")));
    usage_from_token_counts(
        input,
        output,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        None,
    )
}

fn usage_from_anthropic_payload(
    payload: &serde_json::Value,
    input: &str,
    output: &str,
) -> CompletionUsageMetrics {
    let usage = payload.get("usage");
    let input_tokens = parse_u64_value(usage.and_then(|row| row.get("input_tokens")));
    let output_tokens = parse_u64_value(usage.and_then(|row| row.get("output_tokens")));
    let total_tokens = match (input_tokens, output_tokens) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        _ => None,
    };
    usage_from_token_counts(
        input,
        output,
        input_tokens,
        output_tokens,
        total_tokens,
        None,
    )
}

fn usage_from_ollama_payload(
    payload: &serde_json::Value,
    input: &str,
    output: &str,
) -> CompletionUsageMetrics {
    let input_tokens = parse_u64_value(payload.get("prompt_eval_count"));
    let output_tokens = parse_u64_value(payload.get("eval_count"));
    let total_tokens = match (input_tokens, output_tokens) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        _ => None,
    };
    usage_from_token_counts(
        input,
        output,
        input_tokens,
        output_tokens,
        total_tokens,
        None,
    )
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

fn extract_ollama_content(payload: &serde_json::Value) -> Option<String> {
    let content = payload.get("message")?.get("content")?.as_str()?;
    let content = content.trim();
    if content.is_empty() {
        None
    } else {
        Some(content.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::POST;
    use httpmock::MockServer;
    use std::future::Future;
    use std::sync::Mutex;
    use tokio::sync::Mutex as AsyncMutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());
    static ASYNC_ENV_LOCK: AsyncMutex<()> = AsyncMutex::const_new(());

    fn resolve_python_command() -> Option<String> {
        if let Ok(explicit) = std::env::var("PYTHON") {
            let trimmed = explicit.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        for candidate in ["python3", "python"] {
            if std::process::Command::new(candidate)
                .arg("--version")
                .output()
                .is_ok()
            {
                return Some(candidate.to_string());
            }
        }
        None
    }

    fn with_env_vars<T>(values: &[(&str, Option<&str>)], run: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = values
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in values {
            match value {
                Some(raw) => std::env::set_var(key, raw),
                None => std::env::remove_var(key),
            }
        }
        let result = run();
        for (key, value) in previous {
            match value {
                Some(raw) => std::env::set_var(&key, raw),
                None => std::env::remove_var(&key),
            }
        }
        result
    }

    async fn with_env_vars_async<T>(
        values: &[(&str, Option<&str>)],
        run: impl Future<Output = T>,
    ) -> T {
        let _guard = ASYNC_ENV_LOCK.lock().await;
        let previous = values
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in values {
            match value {
                Some(raw) => std::env::set_var(key, raw),
                None => std::env::remove_var(key),
            }
        }
        let result = run.await;
        for (key, value) in previous {
            match value {
                Some(raw) => std::env::set_var(&key, raw),
                None => std::env::remove_var(&key),
            }
        }
        result
    }

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
    async fn anthropic_agent_sdk_headless_executes_local_command() {
        let Some(python) = resolve_python_command() else {
            return;
        };
        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "anthropic".to_string(),
                model_id: "claude-sonnet".to_string(),
                input: "ship patch".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p-headless".to_string()),
                    auth_mode: AGENT_SDK_AUTH_MODE.to_string(),
                    risk_level: "high".to_string(),
                    api_base_url: None,
                    credentials_json: serde_json::json!({
                        "headless_command": python,
                        "headless_args": [
                            "-c",
                            "import sys;print('headless:'+sys.argv[-1])"
                        ],
                        "headless_timeout_ms": 5000
                    })
                    .to_string(),
                }),
            })
            .await
            .expect("anthropic headless completion");

        assert_eq!(response.output_text, "headless:ship patch");
        assert!(response.usage.total_tokens >= 1);
    }

    #[tokio::test]
    async fn anthropic_agent_sdk_headless_timeout_maps_to_provider_timeout() {
        let Some(python) = resolve_python_command() else {
            return;
        };
        let registry = ProviderRegistry::new();
        let error = registry
            .complete(CompletionRequest {
                model_provider: "anthropic".to_string(),
                model_id: "claude-sonnet".to_string(),
                input: "slow job".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p-headless-timeout".to_string()),
                    auth_mode: AGENT_SDK_AUTH_MODE.to_string(),
                    risk_level: "high".to_string(),
                    api_base_url: None,
                    credentials_json: serde_json::json!({
                        "headless_command": python,
                        "headless_args": [
                            "-c",
                            "import time;time.sleep(2);print('done')"
                        ],
                        "headless_timeout_ms": 1000
                    })
                    .to_string(),
                }),
            })
            .await
            .expect_err("expected headless timeout");

        assert!(error
            .to_string()
            .contains("PROVIDER_ERROR:anthropic:TIMEOUT"));
    }

    #[tokio::test]
    async fn provider_auth_is_required_for_external_providers() {
        let registry = ProviderRegistry::new();
        let error = registry
            .complete(CompletionRequest {
                model_provider: "openrouter".to_string(),
                model_id: "gpt-test".to_string(),
                input: "hello".to_string(),
                auth_profile: None,
            })
            .await
            .expect_err("expected missing auth error");

        assert!(error
            .to_string()
            .contains("PROVIDER_ERROR:openrouter:AUTH_REQUIRED"));
    }

    #[test]
    fn provider_capabilities_contract_covers_core_providers() {
        let registry = ProviderRegistry::new();
        let all = registry.list_capabilities();
        assert!(all.iter().any(|item| item.provider == "openai"));
        assert!(all.iter().any(|item| item.provider == "anthropic"));
        assert!(all.iter().any(|item| item.provider == "openrouter"));
        assert!(all.iter().any(|item| item.provider == "ollama"));
        assert!(all.iter().any(|item| item.provider == "vllm"));
        assert!(all.iter().any(|item| item.provider == "lmstudio"));

        let openai = registry
            .capabilities("openai")
            .expect("missing openai capabilities");
        assert!(openai.supports_streaming);
        assert!(openai.supports_json_mode);
        assert!(openai.error_classes.contains(&"AUTH_REQUIRED".to_string()));
        assert!(openai
            .retryable_error_classes
            .contains(&"TIMEOUT".to_string()));
    }

    #[test]
    fn parse_provider_error_class_handles_normalized_provider_error_prefix() {
        let class = parse_provider_error_class("PROVIDER_ERROR:openai:RATE_LIMITED:status=429");
        assert_eq!(class, ProviderErrorClass::RateLimited);
        assert!(provider_error_retryable(class));

        let unknown = parse_provider_error_class("PROVIDER_ERROR:openai:SOMETHING_ELSE");
        assert_eq!(unknown, ProviderErrorClass::InternalError);
        assert!(!provider_error_retryable(unknown));
    }

    #[tokio::test]
    async fn openrouter_provider_returns_output_with_auth_profile() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "message": {
                                "content": "openrouter says hi"
                            }
                        }
                    ]
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "openrouter".to_string(),
                model_id: "openrouter/test-model".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p3".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: Some(server.base_url()),
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            })
            .await
            .expect("openrouter completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "openrouter says hi");
    }

    #[tokio::test]
    async fn ollama_provider_returns_output_with_optional_auth_profile() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/api/chat");
                then.status(200).json_body(json!({
                    "message": {
                        "content": "ollama says hi"
                    }
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "ollama".to_string(),
                model_id: "llama3.2".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p4".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: Some(server.base_url()),
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            })
            .await
            .expect("ollama completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "ollama says hi");
    }

    #[tokio::test]
    async fn vllm_provider_returns_output_with_optional_auth_profile() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "message": {
                                "content": "vllm says hi"
                            }
                        }
                    ]
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "vllm".to_string(),
                model_id: "vllm-model".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p5".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: Some(server.base_url()),
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            })
            .await
            .expect("vllm completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "vllm says hi");
    }

    #[test]
    fn provider_api_base_url_uses_env_override_when_profile_base_is_missing() {
        with_env_vars(
            &[
                (
                    PROVIDER_VLLM_API_BASE_URL_ENV,
                    Some("http://env-vllm:9000/"),
                ),
                (
                    PROVIDER_OLLAMA_API_BASE_URL_ENV,
                    Some("http://env-ollama:9500/"),
                ),
            ],
            || {
                assert_eq!(
                    provider_api_base_url(
                        None,
                        PROVIDER_VLLM_API_BASE_URL_ENV,
                        PROVIDER_VLLM_DEFAULT_API_BASE_URL,
                    ),
                    "http://env-vllm:9000"
                );
                assert_eq!(
                    provider_api_base_url(
                        None,
                        PROVIDER_OLLAMA_API_BASE_URL_ENV,
                        PROVIDER_OLLAMA_DEFAULT_API_BASE_URL,
                    ),
                    "http://env-ollama:9500"
                );
            },
        );
    }

    #[tokio::test]
    async fn vllm_provider_uses_env_api_base_when_profile_base_is_missing() {
        let server = MockServer::start_async().await;
        let server_base_url = server.base_url();
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "message": {
                                "content": "vllm env says hi"
                            }
                        }
                    ]
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = with_env_vars_async(
            &[(
                PROVIDER_VLLM_API_BASE_URL_ENV,
                Some(server_base_url.as_str()),
            )],
            registry.complete(CompletionRequest {
                model_provider: "vllm".to_string(),
                model_id: "vllm-model".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p5-env".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: None,
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            }),
        )
        .await
        .expect("vllm completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "vllm env says hi");
    }

    #[tokio::test]
    async fn ollama_provider_uses_env_api_base_when_profile_base_is_missing() {
        let server = MockServer::start_async().await;
        let server_base_url = server.base_url();
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/api/chat");
                then.status(200).json_body(json!({
                    "message": {
                        "content": "ollama env says hi"
                    }
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = with_env_vars_async(
            &[(
                PROVIDER_OLLAMA_API_BASE_URL_ENV,
                Some(server_base_url.as_str()),
            )],
            registry.complete(CompletionRequest {
                model_provider: "ollama".to_string(),
                model_id: "llama3.2".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p4-env".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: None,
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            }),
        )
        .await
        .expect("ollama completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "ollama env says hi");
    }

    #[tokio::test]
    async fn lmstudio_provider_returns_output_with_optional_auth_profile() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST).path("/v1/chat/completions");
                then.status(200).json_body(json!({
                    "choices": [
                        {
                            "message": {
                                "content": "lmstudio says hi"
                            }
                        }
                    ]
                }));
            })
            .await;

        let registry = ProviderRegistry::new();
        let response = registry
            .complete(CompletionRequest {
                model_provider: "lmstudio".to_string(),
                model_id: "lmstudio-model".to_string(),
                input: "hello".to_string(),
                auth_profile: Some(ProviderAuthProfile {
                    auth_profile_id: Some("p6".to_string()),
                    auth_mode: "api_key".to_string(),
                    risk_level: "low".to_string(),
                    api_base_url: Some(server.base_url()),
                    credentials_json: r#"{"api_key":"test-token"}"#.to_string(),
                }),
            })
            .await
            .expect("lmstudio completion");

        mock.assert_async().await;
        assert_eq!(response.output_text, "lmstudio says hi");
    }
}
