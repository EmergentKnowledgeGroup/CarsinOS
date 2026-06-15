use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use thiserror::Error;

pub const DEFAULT_MAX_OUTPUT_CHARS: usize = 20_000;
pub const DEFAULT_MAX_READ_BYTES: usize = 128 * 1024;
const DEFAULT_TOOL_NETWORK_ALLOWLIST: &[&str] = &[
    "api.duckduckgo.com",
    "duckduckgo.com",
    "html.duckduckgo.com",
    "lite.duckduckgo.com",
    "localhost",
    "127.0.0.1",
];
const DEFAULT_TOOL_ALLOWED_BINARIES: &[&str] = &[
    "cat", "echo", "git", "head", "ls", "pwd", "printf", "rg", "sed", "sleep", "tail", "wc",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolNetworkPolicy {
    Allowlist,
    DenyAll,
}

#[derive(Debug, Clone)]
pub struct ToolSandboxPolicy {
    pub allowed_roots: Vec<PathBuf>,
    pub allowed_binaries: HashSet<String>,
    pub network_policy: ToolNetworkPolicy,
    pub network_allowlist: Vec<String>,
}

impl ToolSandboxPolicy {
    pub fn from_env() -> Self {
        let mut allowed_roots = parse_csv_env("CARSINOS_TOOL_ALLOWED_ROOTS")
            .into_iter()
            .filter_map(|raw| canonicalize_if_exists_or_absolute(&PathBuf::from(raw)))
            .collect::<Vec<_>>();
        if allowed_roots.is_empty() {
            if let Ok(cwd) = std::env::current_dir() {
                if let Some(canonical) = canonicalize_if_exists_or_absolute(&cwd) {
                    allowed_roots.push(canonical);
                }
            }
            if let Some(temp_root) = canonicalize_if_exists_or_absolute(&std::env::temp_dir()) {
                allowed_roots.push(temp_root);
            }
        }
        if allowed_roots.is_empty() {
            allowed_roots.push(PathBuf::from("."));
        }

        let mut allowed_binaries = parse_csv_env("CARSINOS_TOOL_ALLOWED_BINARIES")
            .into_iter()
            .map(|item| item.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if allowed_binaries.is_empty() {
            allowed_binaries.extend(
                DEFAULT_TOOL_ALLOWED_BINARIES
                    .iter()
                    .map(|item| item.to_string()),
            );
        }

        let network_policy = match std::env::var("CARSINOS_TOOL_NETWORK_POLICY")
            .unwrap_or_else(|_| "allowlist".to_string())
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "deny_all" => ToolNetworkPolicy::DenyAll,
            _ => ToolNetworkPolicy::Allowlist,
        };

        let mut network_allowlist = parse_csv_env("CARSINOS_TOOL_NETWORK_ALLOWLIST");
        if network_allowlist.is_empty() {
            network_allowlist = DEFAULT_TOOL_NETWORK_ALLOWLIST
                .iter()
                .map(|item| item.to_string())
                .collect();
        }

        Self {
            allowed_roots,
            allowed_binaries,
            network_policy,
            network_allowlist,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    Exec,
    Process,
    FsRead,
    FsWrite,
    WebSearch,
    WebFetch,
    ChannelAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tool", content = "args", rename_all = "snake_case")]
pub enum ToolRequest {
    Exec(ExecRequest),
    Process(ProcessRequest),
    FsRead(FsReadRequest),
    FsWrite(FsWriteRequest),
    WebSearch(WebSearchRequest),
    WebFetch(WebFetchRequest),
    ChannelAction(ChannelActionRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub command: String,
    pub workdir: Option<String>,
    pub env: Option<BTreeMap<String, String>>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessRequest {
    pub action: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsReadRequest {
    pub path: String,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsWriteRequest {
    pub path: String,
    pub content: String,
    pub mode: FsWriteMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FsWriteMode {
    Create,
    Overwrite,
    Append,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchRequest {
    pub query: String,
    pub count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebFetchRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelActionRequest {
    pub provider: String,
    pub action: String,
    pub target: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub reaction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool: ToolName,
    pub output: serde_json::Value,
    pub truncated: bool,
}

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("tool not implemented: {0}")]
    NotImplemented(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("tool failed: {0}")]
    Failed(String),
}

pub trait ToolRunner {
    fn run(&self, request: ToolRequest) -> Result<ToolResult, ToolError>;
}

#[derive(Debug, Clone)]
pub struct LocalToolRunner {
    pub max_output_chars: usize,
    pub max_read_bytes: usize,
    pub sandbox: ToolSandboxPolicy,
}

impl Default for LocalToolRunner {
    fn default() -> Self {
        Self::from_env()
    }
}

impl LocalToolRunner {
    pub fn from_env() -> Self {
        Self {
            max_output_chars: parse_usize_env(
                "CARSINOS_TOOL_MAX_OUTPUT_CHARS",
                DEFAULT_MAX_OUTPUT_CHARS,
            ),
            max_read_bytes: parse_usize_env("CARSINOS_TOOL_MAX_READ_BYTES", DEFAULT_MAX_READ_BYTES),
            sandbox: ToolSandboxPolicy::from_env(),
        }
    }

    pub fn with_sandbox_policy(sandbox: ToolSandboxPolicy) -> Self {
        Self {
            max_output_chars: DEFAULT_MAX_OUTPUT_CHARS,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            sandbox,
        }
    }
}

impl ToolRunner for LocalToolRunner {
    fn run(&self, request: ToolRequest) -> Result<ToolResult, ToolError> {
        match request {
            ToolRequest::Exec(args) => self.exec(args),
            ToolRequest::FsRead(args) => self.fs_read(args),
            ToolRequest::FsWrite(args) => self.fs_write(args),
            ToolRequest::Process(args) => self.process(args),
            ToolRequest::WebSearch(args) => self.web_search(args),
            ToolRequest::WebFetch(args) => self.web_fetch(args),
            ToolRequest::ChannelAction(args) => self.channel_action(args),
        }
    }
}

impl LocalToolRunner {
    fn exec(&self, args: ExecRequest) -> Result<ToolResult, ToolError> {
        if args.command.trim().is_empty() {
            return Err(ToolError::InvalidRequest(
                "exec command cannot be empty".to_string(),
            ));
        }

        let (binary, exec_args) = parse_exec_command(args.command.trim())?;
        self.ensure_binary_allowed(&binary)?;

        let mut command = std::process::Command::new(&binary);
        command.args(exec_args);
        if let Some(workdir) = args.workdir {
            let canonical_workdir = self.resolve_workdir(&workdir)?;
            command.current_dir(canonical_workdir);
        }
        if let Some(env) = args.env {
            command.envs(env);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command.spawn()?;
        let timed_out = match args.timeout_ms {
            Some(timeout_ms) => wait_with_timeout_and_kill(&mut child, timeout_ms)?,
            None => false,
        };
        let output = child.wait_with_output()?;
        let (stdout, stdout_truncated) = truncate_text(
            String::from_utf8_lossy(&output.stdout).as_ref(),
            self.max_output_chars,
        );
        let (stderr, stderr_truncated) = truncate_text(
            String::from_utf8_lossy(&output.stderr).as_ref(),
            self.max_output_chars,
        );

        Ok(ToolResult {
            tool: ToolName::Exec,
            output: json!({
                "exit_code": output.status.code(),
                "success": output.status.success(),
                "timed_out": timed_out,
                "stdout": stdout,
                "stderr": stderr
            }),
            truncated: stdout_truncated || stderr_truncated,
        })
    }

    fn fs_read(&self, args: FsReadRequest) -> Result<ToolResult, ToolError> {
        if args.path.trim().is_empty() {
            return Err(ToolError::InvalidRequest(
                "fs_read path cannot be empty".to_string(),
            ));
        }
        let path = self.resolve_read_path(&args.path)?;
        let max_bytes = args.max_bytes.unwrap_or(self.max_read_bytes);
        let bytes = std::fs::read(&path)?;
        let truncated = bytes.len() > max_bytes;
        let body = if truncated {
            &bytes[..max_bytes]
        } else {
            &bytes[..]
        };
        let text = String::from_utf8_lossy(body).to_string();

        Ok(ToolResult {
            tool: ToolName::FsRead,
            output: json!({
                "path": path.display().to_string(),
                "bytes": bytes.len(),
                "content": text
            }),
            truncated,
        })
    }

    fn fs_write(&self, args: FsWriteRequest) -> Result<ToolResult, ToolError> {
        if args.path.trim().is_empty() {
            return Err(ToolError::InvalidRequest(
                "fs_write path cannot be empty".to_string(),
            ));
        }
        let path = self.resolve_write_path(&args.path)?;

        let mut options = OpenOptions::new();
        options.write(true);
        match args.mode {
            FsWriteMode::Create => {
                options.create_new(true);
            }
            FsWriteMode::Overwrite => {
                options.create(true).truncate(true);
            }
            FsWriteMode::Append => {
                options.create(true).append(true);
            }
        }

        let mut file = options.open(&path)?;
        file.write_all(args.content.as_bytes())?;

        Ok(ToolResult {
            tool: ToolName::FsWrite,
            output: json!({
                "path": path.display().to_string(),
                "bytes_written": args.content.len()
            }),
            truncated: false,
        })
    }

    fn process(&self, args: ProcessRequest) -> Result<ToolResult, ToolError> {
        let action = args.action.trim().to_ascii_lowercase();
        if action.is_empty() {
            return Err(ToolError::InvalidRequest(
                "process action cannot be empty".to_string(),
            ));
        }

        match action.as_str() {
            "list" => {
                let raw = list_processes_output()?;
                let (output, truncated) = truncate_text(&raw, self.max_output_chars);
                Ok(ToolResult {
                    tool: ToolName::Process,
                    output: json!({
                        "action": "list",
                        "output": output
                    }),
                    truncated,
                })
            }
            "status" => {
                let pid = parse_process_id(args.session_id)?;
                let exists = process_exists(pid)?;
                Ok(ToolResult {
                    tool: ToolName::Process,
                    output: json!({
                        "action": "status",
                        "pid": pid,
                        "exists": exists
                    }),
                    truncated: false,
                })
            }
            "terminate" => {
                let pid = parse_process_id(args.session_id)?;
                let terminated = terminate_process(pid)?;
                Ok(ToolResult {
                    tool: ToolName::Process,
                    output: json!({
                        "action": "terminate",
                        "pid": pid,
                        "terminated": terminated
                    }),
                    truncated: false,
                })
            }
            _ => Err(ToolError::InvalidRequest(format!(
                "unsupported process action: {} (expected list|status|terminate)",
                action
            ))),
        }
    }

    fn web_search(&self, args: WebSearchRequest) -> Result<ToolResult, ToolError> {
        let query = trim_matching_quotes(args.query.trim());
        if query.is_empty() {
            return Err(ToolError::InvalidRequest(
                "web_search query cannot be empty".to_string(),
            ));
        }
        let count = args.count.unwrap_or(5).clamp(1, 10);
        let base_url = std::env::var("CARSINOS_WEB_SEARCH_BASE_URL")
            .unwrap_or_else(|_| "https://api.duckduckgo.com/".to_string());
        self.ensure_network_allowed(&base_url)?;
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|err| {
                ToolError::Failed(format!("failed to build web_search client: {err}"))
            })?;
        let response = client
            .get(&base_url)
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .map_err(|err| ToolError::Failed(format!("web_search request failed: {err}")))?;
        if !response.status().is_success() {
            return Err(ToolError::Failed(format!(
                "web_search HTTP {}",
                response.status().as_u16()
            )));
        }

        let payload: serde_json::Value = response
            .json()
            .map_err(|err| ToolError::Failed(format!("web_search JSON parse failed: {err}")))?;
        let mut results = Vec::new();
        let abstract_text = payload
            .get("AbstractText")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        let abstract_url = payload
            .get("AbstractURL")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim();
        if !abstract_text.is_empty() && !abstract_url.is_empty() && results.len() < count {
            results.push(json!({
                "title": abstract_text,
                "url": abstract_url,
                "snippet": abstract_text
            }));
        }
        if let Some(topics) = payload
            .get("RelatedTopics")
            .and_then(|value| value.as_array())
        {
            collect_duckduckgo_topics(topics, count, &mut results);
        }
        if results.is_empty() {
            let html_base_url = std::env::var("CARSINOS_WEB_SEARCH_HTML_BASE_URL")
                .unwrap_or_else(|_| "https://duckduckgo.com/html/".to_string());
            self.ensure_network_allowed(&html_base_url)?;
            let html_response = client
                .get(&html_base_url)
                .query(&[("q", query)])
                .send()
                .map_err(|err| {
                    ToolError::Failed(format!("web_search HTML fallback request failed: {err}"))
                })?;
            if !html_response.status().is_success() {
                return Err(ToolError::Failed(format!(
                    "web_search HTML fallback HTTP {}",
                    html_response.status().as_u16()
                )));
            }
            let html = html_response.text().map_err(|err| {
                ToolError::Failed(format!("web_search HTML fallback read failed: {err}"))
            })?;
            results.extend(collect_duckduckgo_html_results(&html, count));
        }

        Ok(ToolResult {
            tool: ToolName::WebSearch,
            output: json!({
                "query": query,
                "count": count,
                "results": results
            }),
            truncated: false,
        })
    }

    fn web_fetch(&self, args: WebFetchRequest) -> Result<ToolResult, ToolError> {
        let url = args.url.trim();
        if url.is_empty() {
            return Err(ToolError::InvalidRequest(
                "web_fetch url cannot be empty".to_string(),
            ));
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(ToolError::InvalidRequest(
                "web_fetch url must start with http:// or https://".to_string(),
            ));
        }
        self.ensure_network_allowed(url)?;

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|err| ToolError::Failed(format!("failed to build web_fetch client: {err}")))?;
        let response = client
            .get(url)
            .send()
            .map_err(|err| ToolError::Failed(format!("web_fetch request failed: {err}")))?;
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());
        let body = response
            .text()
            .map_err(|err| ToolError::Failed(format!("web_fetch response read failed: {err}")))?;
        let (body, truncated) = truncate_text(&body, self.max_output_chars);

        if !status.is_success() {
            return Err(ToolError::Failed(format!(
                "web_fetch HTTP {} body={}",
                status.as_u16(),
                body
            )));
        }

        Ok(ToolResult {
            tool: ToolName::WebFetch,
            output: json!({
                "url": url,
                "status_code": status.as_u16(),
                "content_type": content_type,
                "body": body
            }),
            truncated,
        })
    }

    fn resolve_workdir(&self, raw: &str) -> Result<PathBuf, ToolError> {
        let candidate = normalize_absolute_path(Path::new(raw))
            .map_err(|err| ToolError::InvalidRequest(format!("invalid workdir: {err}")))?;
        let canonical = std::fs::canonicalize(&candidate).map_err(|_| {
            ToolError::InvalidRequest("workdir must exist inside allowed roots".to_string())
        })?;
        self.ensure_allowed_root(&canonical)?;
        Ok(canonical)
    }

    fn resolve_read_path(&self, raw: &str) -> Result<PathBuf, ToolError> {
        let candidate = normalize_absolute_path(Path::new(raw))
            .map_err(|err| ToolError::InvalidRequest(format!("invalid fs_read path: {err}")))?;
        let canonical = std::fs::canonicalize(&candidate).map_err(|err| {
            ToolError::InvalidRequest(format!("fs_read path resolution failed: {err}"))
        })?;
        self.ensure_allowed_root(&canonical)?;
        Ok(canonical)
    }

    fn resolve_write_path(&self, raw: &str) -> Result<PathBuf, ToolError> {
        let candidate = normalize_absolute_path(Path::new(raw))
            .map_err(|err| ToolError::InvalidRequest(format!("invalid fs_write path: {err}")))?;
        let parent = candidate.parent().ok_or_else(|| {
            ToolError::InvalidRequest("fs_write path must include a parent directory".to_string())
        })?;
        let canonical_parent = std::fs::canonicalize(parent).map_err(|_| {
            ToolError::InvalidRequest(
                "fs_write parent directory must already exist inside allowed roots".to_string(),
            )
        })?;
        self.ensure_allowed_root(&canonical_parent)?;
        let file_name = candidate.file_name().ok_or_else(|| {
            ToolError::InvalidRequest("fs_write path must include a file name".to_string())
        })?;
        Ok(canonical_parent.join(file_name))
    }

    fn ensure_allowed_root(&self, candidate: &Path) -> Result<(), ToolError> {
        if self
            .sandbox
            .allowed_roots
            .iter()
            .any(|root| candidate.starts_with(root))
        {
            return Ok(());
        }
        Err(ToolError::PolicyDenied(format!(
            "path '{}' is outside allowed roots",
            candidate.display()
        )))
    }

    fn ensure_binary_allowed(&self, binary: &str) -> Result<(), ToolError> {
        if binary.contains('/') || binary.contains('\\') || binary.contains(':') {
            return Err(ToolError::PolicyDenied(
                "exec binary must be an allowlisted command name, not a path".to_string(),
            ));
        }
        let normalized = binary.to_ascii_lowercase();
        if self.sandbox.allowed_binaries.contains(&normalized) {
            return Ok(());
        }
        Err(ToolError::PolicyDenied(format!(
            "exec binary '{}' is not allowlisted",
            normalized
        )))
    }

    fn ensure_network_allowed(&self, raw_url: &str) -> Result<(), ToolError> {
        let parsed = reqwest::Url::parse(raw_url)
            .map_err(|_| ToolError::InvalidRequest("url must be valid".to_string()))?;
        let host = parsed
            .host_str()
            .map(|value| value.to_ascii_lowercase())
            .ok_or_else(|| ToolError::InvalidRequest("url host is required".to_string()))?;

        match self.sandbox.network_policy {
            ToolNetworkPolicy::DenyAll => Err(ToolError::PolicyDenied(
                "network calls are disabled by tool policy".to_string(),
            )),
            ToolNetworkPolicy::Allowlist => {
                if self
                    .sandbox
                    .network_allowlist
                    .iter()
                    .map(|value| value.trim().to_ascii_lowercase())
                    .any(|allowed| host == allowed || host.ends_with(&format!(".{allowed}")))
                {
                    Ok(())
                } else {
                    Err(ToolError::PolicyDenied(format!(
                        "network host '{}' is not allowlisted",
                        host
                    )))
                }
            }
        }
    }

    fn channel_action(&self, _args: ChannelActionRequest) -> Result<ToolResult, ToolError> {
        Err(ToolError::NotImplemented(
            "channel_action must be handled by gateway channel adapters".to_string(),
        ))
    }
}

fn parse_usize_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_csv_env(name: &str) -> Vec<String> {
    std::env::var(name)
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn trim_matching_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return trimmed[1..trimmed.len() - 1].trim();
        }
    }
    trimmed
}

fn canonicalize_if_exists_or_absolute(path: &Path) -> Option<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path).ok();
    }
    normalize_absolute_path(path).ok()
}

fn normalize_absolute_path(path: &Path) -> Result<PathBuf, std::io::Error> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn parse_exec_command(raw: &str) -> Result<(String, Vec<String>), ToolError> {
    let command = raw.trim();
    if command.is_empty() {
        return Err(ToolError::InvalidRequest(
            "exec command cannot be empty".to_string(),
        ));
    }
    if contains_unsafe_shell_operator(command) {
        return Err(ToolError::PolicyDenied(
            "exec command contains disallowed shell operators".to_string(),
        ));
    }

    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escape = false;

    for ch in command.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
            }
            ' ' | '\t' => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if quote.is_some() {
        return Err(ToolError::InvalidRequest(
            "exec command has unclosed quotes".to_string(),
        ));
    }
    if escape {
        return Err(ToolError::InvalidRequest(
            "exec command has trailing escape".to_string(),
        ));
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    if tokens.is_empty() {
        return Err(ToolError::InvalidRequest(
            "exec command cannot be empty".to_string(),
        ));
    }
    let binary = tokens.remove(0);
    Ok((binary, tokens))
}

fn contains_unsafe_shell_operator(command: &str) -> bool {
    command.contains(';')
        || command.contains('|')
        || command.contains('&')
        || command.contains('>')
        || command.contains('<')
        || command.contains('\n')
        || command.contains('\r')
        || command.contains('`')
}

fn parse_process_id(value: Option<String>) -> Result<u32, ToolError> {
    let raw = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ToolError::InvalidRequest("process session_id (pid) is required".to_string())
        })?;
    let pid = raw.parse::<u32>().map_err(|_| {
        ToolError::InvalidRequest("process pid must be a positive integer".to_string())
    })?;
    if pid == 0 {
        return Err(ToolError::InvalidRequest(
            "process pid must be > 0".to_string(),
        ));
    }
    Ok(pid)
}

#[cfg(unix)]
fn list_processes_output() -> Result<String, ToolError> {
    let output = std::process::Command::new("sh")
        .arg("-lc")
        .arg("ps -eo pid,ppid,state,etime,command | head -n 200")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(text)
}

#[cfg(windows)]
fn list_processes_output() -> Result<String, ToolError> {
    let output = std::process::Command::new("tasklist")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(text)
}

#[cfg(unix)]
fn process_exists(pid: u32) -> Result<bool, ToolError> {
    let output = std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    Ok(output.status.success())
}

#[cfg(windows)]
fn process_exists(pid: u32) -> Result<bool, ToolError> {
    let output = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}")])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.contains(&pid.to_string()))
}

#[cfg(unix)]
fn terminate_process(pid: u32) -> Result<bool, ToolError> {
    let output = std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    Ok(output.status.success())
}

#[cfg(windows)]
fn terminate_process(pid: u32) -> Result<bool, ToolError> {
    let output = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    Ok(output.status.success())
}

fn collect_duckduckgo_topics(
    topics: &[serde_json::Value],
    limit: usize,
    out: &mut Vec<serde_json::Value>,
) {
    for item in topics {
        if out.len() >= limit {
            return;
        }
        if let (Some(text), Some(url)) = (
            item.get("Text").and_then(|value| value.as_str()),
            item.get("FirstURL").and_then(|value| value.as_str()),
        ) {
            out.push(json!({
                "title": text.split(" - ").next().unwrap_or(text),
                "url": url,
                "snippet": text
            }));
            continue;
        }
        if let Some(nested) = item.get("Topics").and_then(|value| value.as_array()) {
            collect_duckduckgo_topics(nested, limit, out);
        }
    }
}

fn collect_duckduckgo_html_results(html: &str, limit: usize) -> Vec<serde_json::Value> {
    let mut results = Vec::new();
    let mut cursor = 0;
    while results.len() < limit {
        let Some(class_pos) = html[cursor..].find("result__a") else {
            break;
        };
        let class_pos = cursor + class_pos;
        let Some(anchor_start) = html[..class_pos].rfind("<a") else {
            cursor = class_pos + "result__a".len();
            continue;
        };
        let Some(anchor_open_end_rel) = html[anchor_start..].find('>') else {
            break;
        };
        let anchor_open_end = anchor_start + anchor_open_end_rel;
        let Some(anchor_close_rel) = html[anchor_open_end + 1..].find("</a>") else {
            break;
        };
        let anchor_close = anchor_open_end + 1 + anchor_close_rel;
        let tag = &html[anchor_start..=anchor_open_end];
        let title_html = &html[anchor_open_end + 1..anchor_close];
        let title = decode_html_entities(&strip_html_tags(title_html));
        let url = extract_html_attr(tag, "href")
            .map(|href| normalize_duckduckgo_href(&decode_html_entities(&href)))
            .unwrap_or_default();
        let next_anchor = html[anchor_close..]
            .find("result__a")
            .map(|offset| anchor_close + offset)
            .unwrap_or(html.len());
        let snippet = html[anchor_close..next_anchor]
            .find("result__snippet")
            .and_then(|snippet_class_rel| {
                let snippet_class = anchor_close + snippet_class_rel;
                let open_end = html[snippet_class..]
                    .find('>')
                    .map(|offset| snippet_class + offset)?;
                let close = html[open_end + 1..]
                    .find("</")
                    .map(|offset| open_end + 1 + offset)?;
                Some(decode_html_entities(&strip_html_tags(
                    &html[open_end + 1..close],
                )))
            })
            .unwrap_or_default();
        if !title.is_empty() && !url.is_empty() {
            results.push(json!({
                "title": title,
                "url": url,
                "snippet": snippet
            }));
        }
        cursor = anchor_close + "</a>".len();
    }
    results
}

fn extract_html_attr(tag: &str, attr: &str) -> Option<String> {
    for quote in ['"', '\''] {
        let needle = format!("{attr}={quote}");
        if let Some(start) = tag.find(&needle) {
            let value_start = start + needle.len();
            let value_end = tag[value_start..].find(quote)? + value_start;
            return Some(tag[value_start..value_end].to_string());
        }
    }
    None
}

fn normalize_duckduckgo_href(href: &str) -> String {
    let value = href.trim();
    if let Some(uddg_start) = value.find("uddg=") {
        let encoded_start = uddg_start + "uddg=".len();
        let encoded_end = value[encoded_start..]
            .find('&')
            .map(|offset| encoded_start + offset)
            .unwrap_or(value.len());
        return percent_decode(&value[encoded_start..encoded_end]);
    }
    if let Some(protocol_relative) = value.strip_prefix("//") {
        return format!("https://{protocol_relative}");
    }
    value.to_string()
}

fn strip_html_tags(input: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn decode_html_entities(input: &str) -> String {
    let mut output = input
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    while let Some(start) = output.find("&#") {
        let Some(end_rel) = output[start..].find(';') else {
            break;
        };
        let end = start + end_rel;
        let token = &output[start + 2..end];
        let parsed = if let Some(hex) = token.strip_prefix(['x', 'X']) {
            u32::from_str_radix(hex, 16).ok()
        } else {
            token.parse::<u32>().ok()
        };
        let Some(ch) = parsed.and_then(char::from_u32) else {
            break;
        };
        output.replace_range(start..=end, &ch.to_string());
    }
    output
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) =
                (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
            {
                output.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn truncate_text(input: &str, max_chars: usize) -> (String, bool) {
    if input.chars().count() <= max_chars {
        return (input.to_string(), false);
    }
    let truncated = input.chars().take(max_chars).collect::<String>();
    (truncated, true)
}

fn wait_with_timeout_and_kill(
    child: &mut std::process::Child,
    timeout_ms: u64,
) -> Result<bool, ToolError> {
    let timeout_ms = timeout_ms.max(1);
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        if child.try_wait()?.is_some() {
            return Ok(false);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn exec_runs_basic_command() {
        let runner = LocalToolRunner::default();
        let result = runner
            .run(ToolRequest::Exec(ExecRequest {
                command: "printf 'ok'".to_string(),
                workdir: None,
                env: None,
                timeout_ms: None,
            }))
            .expect("exec should succeed");

        assert_eq!(result.tool, ToolName::Exec);
        assert_eq!(result.output["success"], true);
        assert_eq!(result.output["timed_out"], false);
        assert_eq!(result.output["stdout"], "ok");
    }

    #[test]
    fn fs_write_then_read_round_trip() {
        let runner = LocalToolRunner::default();
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("file.txt");
        let path_str = path.display().to_string();

        runner
            .run(ToolRequest::FsWrite(FsWriteRequest {
                path: path_str.clone(),
                content: "hello world".to_string(),
                mode: FsWriteMode::Overwrite,
            }))
            .expect("write should succeed");

        let read = runner
            .run(ToolRequest::FsRead(FsReadRequest {
                path: path_str,
                max_bytes: None,
            }))
            .expect("read should succeed");
        assert_eq!(read.output["content"], "hello world");
    }

    #[test]
    fn exec_output_is_truncated_when_limit_is_small() {
        let runner = LocalToolRunner {
            max_output_chars: 5,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            sandbox: ToolSandboxPolicy::from_env(),
        };
        let result = runner
            .run(ToolRequest::Exec(ExecRequest {
                command: "printf '1234567890'".to_string(),
                workdir: None,
                env: None,
                timeout_ms: None,
            }))
            .expect("exec should succeed");

        assert_eq!(result.truncated, true);
        assert_eq!(result.output["stdout"], "12345");
    }

    #[test]
    fn exec_timeout_kills_long_running_command() {
        let runner = LocalToolRunner::default();
        let result = runner
            .run(ToolRequest::Exec(ExecRequest {
                command: "sleep 1".to_string(),
                workdir: None,
                env: None,
                timeout_ms: Some(10),
            }))
            .expect("exec should return result even on timeout");

        assert_eq!(result.output["timed_out"], true);
    }

    #[test]
    fn process_list_and_status_work() {
        let runner = LocalToolRunner::default();

        let listed = runner
            .run(ToolRequest::Process(ProcessRequest {
                action: "list".to_string(),
                session_id: None,
            }))
            .expect("process list should succeed");
        assert_eq!(listed.tool, ToolName::Process);
        assert_eq!(listed.output["action"], "list");
        assert!(listed.output["output"].as_str().unwrap_or_default().len() > 0);

        let status = runner
            .run(ToolRequest::Process(ProcessRequest {
                action: "status".to_string(),
                session_id: Some(std::process::id().to_string()),
            }))
            .expect("process status should succeed");
        assert_eq!(status.output["action"], "status");
        assert_eq!(status.output["exists"], true);
    }

    #[test]
    fn web_fetch_reads_http_response() {
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/fetch");
            then.status(200)
                .header("content-type", "text/plain")
                .body("hello fetch");
        });

        let runner = LocalToolRunner::default();
        let result = runner
            .run(ToolRequest::WebFetch(WebFetchRequest {
                url: format!("{}/fetch", server.base_url()),
            }))
            .expect("web_fetch should succeed");
        mock.assert();
        assert_eq!(result.tool, ToolName::WebFetch);
        assert_eq!(result.output["status_code"], 200);
        assert_eq!(result.output["body"], "hello fetch");
    }

    #[test]
    fn web_fetch_truncates_large_response_to_runner_limit() {
        let server = MockServer::start();
        let body = "x".repeat(128);
        let mock = server.mock(|when, then| {
            when.method(GET).path("/large");
            then.status(200)
                .header("content-type", "text/plain")
                .body(body);
        });

        let runner = LocalToolRunner {
            max_output_chars: 32,
            ..LocalToolRunner::default()
        };
        let result = runner
            .run(ToolRequest::WebFetch(WebFetchRequest {
                url: format!("{}/large", server.base_url()),
            }))
            .expect("web_fetch should succeed with truncated body");

        mock.assert();
        assert_eq!(result.tool, ToolName::WebFetch);
        assert_eq!(result.truncated, true);
        assert_eq!(result.output["status_code"], 200);
        assert_eq!(result.output["body"].as_str().unwrap().chars().count(), 32);
    }

    #[test]
    fn web_search_parses_results_from_mock_api() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200).json_body(json!({
                "AbstractText": "Primary result",
                "AbstractURL": "https://example.com/primary",
                "RelatedTopics": [
                    {
                        "Text": "Secondary result - details",
                        "FirstURL": "https://example.com/secondary"
                    }
                ]
            }));
        });

        std::env::set_var("CARSINOS_WEB_SEARCH_BASE_URL", server.base_url());
        let runner = LocalToolRunner::default();
        let result = runner
            .run(ToolRequest::WebSearch(WebSearchRequest {
                query: "test query".to_string(),
                count: Some(5),
            }))
            .expect("web_search should succeed");
        std::env::remove_var("CARSINOS_WEB_SEARCH_BASE_URL");

        mock.assert();
        assert_eq!(result.tool, ToolName::WebSearch);
        let results = result.output["results"].as_array().expect("results array");
        assert!(!results.is_empty());
    }

    #[test]
    fn web_search_trims_common_outer_query_quotes() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/")
                .query_param("q", "quoted query")
                .query_param("format", "json");
            then.status(200).json_body(json!({
                "AbstractText": "Quoted query result",
                "AbstractURL": "https://example.com/quoted",
                "RelatedTopics": []
            }));
        });

        std::env::set_var("CARSINOS_WEB_SEARCH_BASE_URL", server.base_url());
        let runner = LocalToolRunner::default();
        let result = runner
            .run(ToolRequest::WebSearch(WebSearchRequest {
                query: "\"quoted query\"".to_string(),
                count: Some(5),
            }))
            .expect("web_search should trim matching quote wrapper");
        std::env::remove_var("CARSINOS_WEB_SEARCH_BASE_URL");

        mock.assert();
        assert_eq!(result.output["query"], "quoted query");
        let results = result.output["results"].as_array().expect("results array");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn web_search_falls_back_to_html_results_when_instant_answer_is_empty() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        let server = MockServer::start();
        let instant = server.mock(|when, then| {
            when.method(GET).path("/");
            then.status(200).json_body(json!({
                "AbstractText": "",
                "AbstractURL": "",
                "RelatedTopics": []
            }));
        });
        let html = server.mock(|when, then| {
            when.method(GET)
                .path("/html/")
                .query_param("q", "LM Studio loaded models endpoint");
            then.status(200).body(
                r#"
                <html>
                  <body>
                    <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Flmstudio.ai%2Fdocs%2Fapi%2Fendpoints%2Frest">LM Studio REST API Reference</a>
                    <a class="result__snippet">Use /v1/models to list loaded models.</a>
                  </body>
                </html>
                "#,
            );
        });

        std::env::set_var("CARSINOS_WEB_SEARCH_BASE_URL", server.base_url());
        std::env::set_var(
            "CARSINOS_WEB_SEARCH_HTML_BASE_URL",
            format!("{}/html/", server.base_url()),
        );
        let runner = LocalToolRunner::default();
        let result = runner
            .run(ToolRequest::WebSearch(WebSearchRequest {
                query: "LM Studio loaded models endpoint".to_string(),
                count: Some(5),
            }))
            .expect("web_search should fall back to HTML results");
        std::env::remove_var("CARSINOS_WEB_SEARCH_BASE_URL");
        std::env::remove_var("CARSINOS_WEB_SEARCH_HTML_BASE_URL");

        instant.assert();
        html.assert();
        let results = result.output["results"].as_array().expect("results array");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["title"], "LM Studio REST API Reference");
        assert_eq!(
            results[0]["url"],
            "https://lmstudio.ai/docs/api/endpoints/rest"
        );
        assert!(results[0]["snippet"]
            .as_str()
            .unwrap_or_default()
            .contains("/v1/models"));
    }

    #[test]
    fn fs_read_is_denied_outside_allowed_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let allowed_root = temp.path().canonicalize().expect("canonical allowed root");
        let outside_temp = tempfile::tempdir().expect("outside temp");
        let outside_path = outside_temp.path().join("outside.txt");
        std::fs::write(&outside_path, "secret").expect("write outside file");

        let runner = LocalToolRunner::with_sandbox_policy(ToolSandboxPolicy {
            allowed_roots: vec![allowed_root],
            allowed_binaries: DEFAULT_TOOL_ALLOWED_BINARIES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            network_policy: ToolNetworkPolicy::Allowlist,
            network_allowlist: vec!["localhost".to_string()],
        });

        let err = runner
            .run(ToolRequest::FsRead(FsReadRequest {
                path: outside_path.display().to_string(),
                max_bytes: None,
            }))
            .expect_err("fs_read outside allowlist should fail");
        assert!(matches!(err, ToolError::PolicyDenied(_)));
    }

    #[test]
    fn exec_rejects_disallowed_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runner = LocalToolRunner::with_sandbox_policy(ToolSandboxPolicy {
            allowed_roots: vec![temp.path().canonicalize().expect("canonical root")],
            allowed_binaries: ["echo"].iter().map(|value| value.to_string()).collect(),
            network_policy: ToolNetworkPolicy::Allowlist,
            network_allowlist: vec!["localhost".to_string()],
        });

        let err = runner
            .run(ToolRequest::Exec(ExecRequest {
                command: "printf 'hello'".to_string(),
                workdir: None,
                env: None,
                timeout_ms: None,
            }))
            .expect_err("disallowed binary must be rejected");
        assert!(matches!(err, ToolError::PolicyDenied(_)));
    }

    #[test]
    fn exec_rejects_path_qualified_allowlisted_binary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runner = LocalToolRunner::with_sandbox_policy(ToolSandboxPolicy {
            allowed_roots: vec![temp.path().canonicalize().expect("canonical root")],
            allowed_binaries: ["git"].iter().map(|value| value.to_string()).collect(),
            network_policy: ToolNetworkPolicy::Allowlist,
            network_allowlist: vec!["localhost".to_string()],
        });

        for command in ["./git status", "/tmp/git status", r"C:\tmp\git status"] {
            let err = runner
                .run(ToolRequest::Exec(ExecRequest {
                    command: command.to_string(),
                    workdir: None,
                    env: None,
                    timeout_ms: None,
                }))
                .expect_err("path-qualified binary must be rejected");
            assert!(matches!(err, ToolError::PolicyDenied(_)), "{command}");
        }
    }

    #[test]
    fn web_fetch_is_denied_when_host_not_allowlisted() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runner = LocalToolRunner::with_sandbox_policy(ToolSandboxPolicy {
            allowed_roots: vec![temp.path().canonicalize().expect("canonical root")],
            allowed_binaries: DEFAULT_TOOL_ALLOWED_BINARIES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            network_policy: ToolNetworkPolicy::Allowlist,
            network_allowlist: vec!["localhost".to_string()],
        });

        let err = runner
            .run(ToolRequest::WebFetch(WebFetchRequest {
                url: "https://example.com/".to_string(),
            }))
            .expect_err("non-allowlisted host must fail");
        assert!(matches!(err, ToolError::PolicyDenied(_)));
    }

    #[test]
    fn web_search_is_denied_when_network_policy_is_deny_all() {
        let _env_guard = ENV_LOCK.lock().expect("env lock");
        let temp = tempfile::tempdir().expect("tempdir");
        let runner = LocalToolRunner::with_sandbox_policy(ToolSandboxPolicy {
            allowed_roots: vec![temp.path().canonicalize().expect("canonical root")],
            allowed_binaries: DEFAULT_TOOL_ALLOWED_BINARIES
                .iter()
                .map(|value| value.to_string())
                .collect(),
            network_policy: ToolNetworkPolicy::DenyAll,
            network_allowlist: Vec::new(),
        });

        let err = runner
            .run(ToolRequest::WebSearch(WebSearchRequest {
                query: "carsinos".to_string(),
                count: Some(3),
            }))
            .expect_err("deny_all network policy must block search");
        assert!(matches!(err, ToolError::PolicyDenied(_)));
    }
}
