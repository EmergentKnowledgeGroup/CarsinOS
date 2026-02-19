use anyhow::{anyhow, Context, Result};
use portpicker::pick_unused_port;
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{header::AUTHORIZATION, HeaderValue};
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[allow(dead_code)]
pub type WsStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

pub struct GatewayProcess {
    child: Child,
    bind: String,
    token: String,
    client: Client,
}

impl GatewayProcess {
    pub async fn spawn(
        state_dir: &Path,
        token: &str,
        operator_allowlist: Option<&str>,
    ) -> Result<Self> {
        Self::spawn_with_env(state_dir, token, operator_allowlist, &[]).await
    }

    pub async fn spawn_with_env(
        state_dir: &Path,
        token: &str,
        operator_allowlist: Option<&str>,
        extra_env: &[(&str, &str)],
    ) -> Result<Self> {
        let port =
            pick_unused_port().ok_or_else(|| anyhow!("failed to pick an unused TCP port"))?;
        let bind = format!("127.0.0.1:{port}");
        let binary = gateway_binary_path()?;

        let mut command = Command::new(binary);
        command
            .env("CARSINOS_GATEWAY_BIND", &bind)
            .env("CARSINOS_GATEWAY_TOKEN", token)
            .env("CARSINOS_STATE_DIR", state_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        if let Some(allowlist) = operator_allowlist {
            command.env("CARSINOS_OPERATOR_ALLOWLIST", allowlist);
        }
        for (key, value) in extra_env {
            command.env(key, value);
        }

        let child = command
            .spawn()
            .context("failed to spawn carsinos-gateway process")?;
        let client = Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .context("failed to create HTTP client")?;

        let mut process = Self {
            child,
            bind,
            token: token.to_string(),
            client,
        };
        process.wait_until_ready().await?;
        Ok(process)
    }

    pub fn request(&self, method: Method, path: impl AsRef<str>) -> RequestBuilder {
        self.client
            .request(method, format!("{}{}", self.http_base(), path.as_ref()))
            .bearer_auth(&self.token)
    }

    #[allow(dead_code)]
    pub fn request_with_operator(
        &self,
        method: Method,
        path: impl AsRef<str>,
        operator_id: &str,
    ) -> RequestBuilder {
        self.request(method, path)
            .header("x-operator-id", operator_id)
    }

    #[allow(dead_code)]
    pub async fn connect_ws(&self) -> Result<WsStream> {
        let url = format!("ws://{}/api/v1/ws", self.bind);
        let mut request = url
            .into_client_request()
            .context("failed to build websocket request")?;
        request.headers_mut().insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .context("failed to build websocket auth header")?,
        );

        let (stream, response) = connect_async(request)
            .await
            .context("websocket connect failed")?;
        if response.status() != StatusCode::SWITCHING_PROTOCOLS {
            return Err(anyhow!(
                "unexpected websocket status code: {}",
                response.status()
            ));
        }
        Ok(stream)
    }

    pub fn http_base(&self) -> String {
        format!("http://{}", self.bind)
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    async fn wait_until_ready(&mut self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(12);
        let health_url = format!("{}/api/v1/health", self.http_base());

        while Instant::now() < deadline {
            if let Some(status) = self
                .child
                .try_wait()
                .context("failed to check gateway process state")?
            {
                return Err(anyhow!("gateway exited before becoming ready: {status}"));
            }

            match self
                .client
                .get(&health_url)
                .bearer_auth(&self.token)
                .send()
                .await
            {
                Ok(response) if response.status() == StatusCode::OK => return Ok(()),
                Ok(_) | Err(_) => sleep(Duration::from_millis(100)).await,
            }
        }

        Err(anyhow!("gateway did not become ready before timeout"))
    }
}

impl Drop for GatewayProcess {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            #[cfg(unix)]
            {
                let pid = self.child.id().to_string();
                let _ = Command::new("kill").arg("-TERM").arg(&pid).status();
                let deadline = Instant::now() + Duration::from_secs(2);
                while Instant::now() < deadline {
                    if let Ok(Some(_)) = self.child.try_wait() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub async fn json_body(response: reqwest::Response) -> Result<Value> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    serde_json::from_str(&body)
        .with_context(|| format!("invalid JSON body (status {status}): {body}"))
}

fn gateway_binary_path() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_carsinos-gateway") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_carsinos_gateway") {
        return Ok(PathBuf::from(path));
    }

    let current_exe =
        std::env::current_exe().context("failed to resolve current test binary path")?;
    let target_dir = current_exe
        .parent()
        .and_then(|path| path.parent())
        .ok_or_else(|| {
            anyhow!(
                "failed to resolve target directory from {}",
                current_exe.display()
            )
        })?;

    let candidate = target_dir.join("carsinos-gateway");
    #[cfg(windows)]
    {
        let mut candidate = candidate;
        candidate.set_extension("exe");
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    #[cfg(not(windows))]
    if candidate.exists() {
        return Ok(candidate);
    }

    Err(anyhow!(
        "carsinos-gateway binary not found (checked {})",
        candidate.display()
    ))
}
