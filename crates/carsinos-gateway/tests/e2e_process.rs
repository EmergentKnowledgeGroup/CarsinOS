mod common;

use anyhow::{anyhow, Context, Result};
use axum::extract::State as AxumState;
use axum::routing::post;
use axum::{Json, Router};
use common::{json_body, GatewayProcess, WsStream};
use futures_util::future::join_all;
use futures_util::StreamExt;
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::tungstenite::Message;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn full_http_flow_persists_across_restart() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let token = "e2e-token-restart";

    let session_id = {
        let gateway = GatewayProcess::spawn(state_dir.path(), token, None).await?;

        let health = gateway
            .request(Method::GET, "/api/v1/health")
            .send()
            .await
            .context("health request failed")?;
        assert_eq!(health.status(), StatusCode::OK);

        let created_session = gateway
            .request(Method::POST, "/api/v1/sessions")
            .json(&json!({ "title": "restart-flow" }))
            .send()
            .await
            .context("create session request failed")?;
        assert_eq!(created_session.status(), StatusCode::CREATED);
        let created_session_json = json_body(created_session).await?;
        let session_id = created_session_json["session"]["session_id"]
            .as_str()
            .context("missing session_id from create_session response")?
            .to_string();

        let created_message = gateway
            .request(
                Method::POST,
                format!("/api/v1/sessions/{session_id}/messages"),
            )
            .json(&json!({
                "role": "user",
                "content_text": "hello from persistence test"
            }))
            .send()
            .await
            .context("create message request failed")?;
        assert_eq!(created_message.status(), StatusCode::CREATED);

        let created_run = gateway
            .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
            .json(&json!({}))
            .send()
            .await
            .context("create run request failed")?;
        assert_eq!(created_run.status(), StatusCode::CREATED);
        let run_json = json_body(created_run).await?;
        assert_eq!(run_json["run"]["status"], "succeeded");

        let timeline = gateway
            .request(
                Method::GET,
                format!("/api/v1/sessions/{session_id}/messages?limit=10"),
            )
            .send()
            .await
            .context("list messages request failed")?;
        assert_eq!(timeline.status(), StatusCode::OK);
        let timeline_json = json_body(timeline).await?;
        let timeline_items = timeline_json["items"]
            .as_array()
            .context("timeline items missing")?;
        assert_eq!(timeline_items.len(), 2);
        assert_eq!(timeline_items[0]["role"], "user");
        assert_eq!(timeline_items[1]["role"], "assistant");

        session_id
    };

    let gateway = GatewayProcess::spawn(state_dir.path(), token, None).await?;

    let sessions = gateway
        .request(Method::GET, "/api/v1/sessions?limit=50")
        .send()
        .await
        .context("list sessions request after restart failed")?;
    assert_eq!(sessions.status(), StatusCode::OK);
    let sessions_json = json_body(sessions).await?;
    let sessions_items = sessions_json["items"]
        .as_array()
        .context("sessions array missing after restart")?;
    let restored = sessions_items
        .iter()
        .find(|item| item["session_id"] == session_id)
        .context("session was not persisted across restart")?;
    assert_eq!(restored["message_count"], 2);
    assert_eq!(restored["run_count"], 1);

    let restored_messages = gateway
        .request(
            Method::GET,
            format!("/api/v1/sessions/{session_id}/messages?limit=50"),
        )
        .send()
        .await
        .context("list messages after restart failed")?;
    assert_eq!(restored_messages.status(), StatusCode::OK);
    let restored_messages_json = json_body(restored_messages).await?;
    let restored_items = restored_messages_json["items"]
        .as_array()
        .context("messages array missing after restart")?;
    assert_eq!(restored_items.len(), 2);
    assert_eq!(restored_items[0]["role"], "user");
    assert_eq!(restored_items[1]["role"], "assistant");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn websocket_stream_includes_run_and_approval_events() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-ws", Some("op-1")).await?;
    let mut ws = gateway.connect_ws().await?;

    wait_for_ws_event(&mut ws, "gateway.status", Duration::from_secs(2)).await?;

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "ws-events" }))
        .send()
        .await
        .context("create session request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id")?
        .to_string();

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "produce events"
        }))
        .send()
        .await
        .context("create message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({}))
        .send()
        .await
        .context("create run request failed")?;
    assert_eq!(created_run.status(), StatusCode::CREATED);
    let created_run_json = json_body(created_run).await?;
    let run_id = created_run_json["run"]["run_id"]
        .as_str()
        .context("missing run_id")?
        .to_string();

    let created_approval = gateway
        .request_with_operator(Method::POST, "/api/v1/approvals/request", "op-1")
        .json(&json!({
            "run_id": run_id,
            "tool_name": "exec",
            "request_summary": "ws approval",
            "request_json": { "command": "echo hi" }
        }))
        .send()
        .await
        .context("create approval request failed")?;
    assert_eq!(created_approval.status(), StatusCode::CREATED);
    let approval_json = json_body(created_approval).await?;
    let approval_id = approval_json["approval"]["approval_id"]
        .as_str()
        .context("missing approval_id")?
        .to_string();

    let resolved_approval = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/approvals/{approval_id}/resolve"),
            "op-1",
        )
        .json(&json!({
            "decision": "approve",
            "decided_via": "e2e"
        }))
        .send()
        .await
        .context("resolve approval request failed")?;
    assert_eq!(resolved_approval.status(), StatusCode::OK);

    let mut events = ObservedEvents::default();
    let deadline = Duration::from_secs(5);
    while !events.is_complete() {
        let frame = timeout(deadline, next_ws_event(&mut ws))
            .await
            .context("timed out waiting for websocket event")??;
        events.observe(&frame);
    }

    assert!(events.run_created);
    assert!(events.run_status_running);
    assert!(events.run_status_succeeded);
    assert!(events.run_delta);
    assert!(events.approval_requested);
    assert!(events.approval_resolved);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn operator_allowlist_is_enforced_process_level() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway =
        GatewayProcess::spawn(state_dir.path(), "e2e-token-auth", Some("op-allowed")).await?;

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "allowlist" }))
        .send()
        .await
        .context("create session request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id")?
        .to_string();

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "approval auth test"
        }))
        .send()
        .await
        .context("create message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({}))
        .send()
        .await
        .context("create run request failed")?;
    assert_eq!(created_run.status(), StatusCode::CREATED);
    let run_json = json_body(created_run).await?;
    let run_id = run_json["run"]["run_id"]
        .as_str()
        .context("missing run_id")?
        .to_string();

    let missing_operator = gateway
        .request(Method::POST, "/api/v1/approvals/request")
        .json(&json!({
            "run_id": run_id,
            "tool_name": "exec",
            "request_summary": "missing operator",
            "request_json": { "command": "echo hi" }
        }))
        .send()
        .await
        .context("approval request without operator failed")?;
    assert_eq!(missing_operator.status(), StatusCode::FORBIDDEN);

    let wrong_operator = gateway
        .request_with_operator(Method::POST, "/api/v1/approvals/request", "op-wrong")
        .json(&json!({
            "run_id": run_id,
            "tool_name": "exec",
            "request_summary": "wrong operator",
            "request_json": { "command": "echo hi" }
        }))
        .send()
        .await
        .context("approval request with wrong operator failed")?;
    assert_eq!(wrong_operator.status(), StatusCode::FORBIDDEN);

    let created_approval = gateway
        .request_with_operator(Method::POST, "/api/v1/approvals/request", "op-allowed")
        .json(&json!({
            "run_id": run_id,
            "tool_name": "exec",
            "request_summary": "allowed operator",
            "request_json": { "command": "echo hi" }
        }))
        .send()
        .await
        .context("approval request with allowlisted operator failed")?;
    assert_eq!(created_approval.status(), StatusCode::CREATED);
    let created_approval_json = json_body(created_approval).await?;
    let approval_id = created_approval_json["approval"]["approval_id"]
        .as_str()
        .context("missing approval_id")?
        .to_string();

    let deny_wrong_operator = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/approvals/{approval_id}/resolve"),
            "op-wrong",
        )
        .json(&json!({ "decision": "deny", "decided_via": "e2e" }))
        .send()
        .await
        .context("resolve approval with wrong operator failed")?;
    assert_eq!(deny_wrong_operator.status(), StatusCode::FORBIDDEN);

    let approve_allowlisted = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/approvals/{approval_id}/resolve"),
            "op-allowed",
        )
        .json(&json!({ "decision": "approve", "decided_via": "e2e" }))
        .send()
        .await
        .context("resolve approval with allowlisted operator failed")?;
    assert_eq!(approve_allowlisted.status(), StatusCode::OK);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unauthorized_requests_are_rejected_with_request_ids() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-unauthorized", None).await?;
    let client = reqwest::Client::new();

    let health_response = client
        .get(format!("{}/api/v1/health", gateway.http_base()))
        .send()
        .await
        .context("unauthorized health request failed")?;
    assert_eq!(health_response.status(), StatusCode::UNAUTHORIZED);
    assert!(health_response.headers().get("x-request-id").is_some());

    let sessions_response = client
        .get(format!("{}/api/v1/sessions", gateway.http_base()))
        .send()
        .await
        .context("unauthorized sessions request failed")?;
    assert_eq!(sessions_response.status(), StatusCode::UNAUTHORIZED);
    assert!(sessions_response.headers().get("x-request-id").is_some());

    let authorized_health = gateway
        .request(Method::GET, "/api/v1/health")
        .send()
        .await
        .context("authorized health request failed")?;
    assert_eq!(authorized_health.status(), StatusCode::OK);
    assert!(authorized_health.headers().get("x-request-id").is_some());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unsupported_provider_is_rejected_without_assistant_message() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-provider-fail", None).await?;

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "provider-failure" }))
        .send()
        .await
        .context("create session request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id")?
        .to_string();

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "trigger unsupported provider"
        }))
        .send()
        .await
        .context("create message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({
            "model_provider": "unsupported",
            "model_id": "x"
        }))
        .send()
        .await
        .context("create failing run request failed")?;
    assert_eq!(created_run.status(), StatusCode::BAD_REQUEST);

    let timeline = gateway
        .request(
            Method::GET,
            format!("/api/v1/sessions/{session_id}/messages?limit=50"),
        )
        .send()
        .await
        .context("list timeline request failed")?;
    assert_eq!(timeline.status(), StatusCode::OK);
    let timeline_json = json_body(timeline).await?;
    let items = timeline_json["items"]
        .as_array()
        .context("timeline items missing")?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["role"], "user");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn numquam_http_integration_wires_context_writeback_and_approval_process_level() -> Result<()>
{
    let numquam_stub = NumquamStubServer::spawn().await?;
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let integration_base_url = numquam_stub.base_url.clone();
    let gateway = GatewayProcess::spawn_with_env(
        state_dir.path(),
        "e2e-token-numquam",
        None,
        &[
            ("CARSINOS_NUMQUAM_ENABLED", "1"),
            ("CARSINOS_NUMQUAM_TRANSPORT", "http"),
            ("CARSINOS_NUMQUAM_BASE_URL", integration_base_url.as_str()),
            ("CARSINOS_NUMQUAM_TOKEN", "stub-token"),
            ("CARSINOS_NUMQUAM_TIMEOUT_MS", "3000"),
        ],
    )
    .await?;

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "numquam-process" }))
        .send()
        .await
        .context("create session request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id")?
        .to_string();

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "numquam process integration"
        }))
        .send()
        .await
        .context("create message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({}))
        .send()
        .await
        .context("create run request failed")?;
    assert_eq!(created_run.status(), StatusCode::CREATED);
    let run_json = json_body(created_run).await?;
    assert_eq!(run_json["run"]["status"], "succeeded");
    let usage_json = run_json["run"]["usage_json"]
        .as_str()
        .context("missing usage_json")?;
    let usage_value: serde_json::Value =
        serde_json::from_str(usage_json).context("invalid usage_json payload")?;
    assert_eq!(usage_value["memory"]["enabled"], true);

    let approvals = gateway
        .request(Method::GET, "/api/v1/approvals?status=requested")
        .send()
        .await
        .context("list approvals request failed")?;
    assert_eq!(approvals.status(), StatusCode::OK);
    let approvals_json = json_body(approvals).await?;
    let items = approvals_json["items"]
        .as_array()
        .context("approval items missing")?;
    assert!(items.iter().any(|item| item["kind"] == "memory.writeback"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn high_risk_tool_flow_requires_approval_then_resumes() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway =
        GatewayProcess::spawn(state_dir.path(), "e2e-token-tool-resume", Some("op-1")).await?;

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "tool-approval-resume" }))
        .send()
        .await
        .context("create session request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id")?
        .to_string();

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "tool.exec echo e2e-resume"
        }))
        .send()
        .await
        .context("create message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({}))
        .send()
        .await
        .context("create run request failed")?;
    assert_eq!(created_run.status(), StatusCode::CREATED);
    let run_json = json_body(created_run).await?;
    let run_id = run_json["run"]["run_id"]
        .as_str()
        .context("missing run_id")?
        .to_string();
    assert_eq!(run_json["run"]["status"], "failed");

    let approvals_response = gateway
        .request(Method::GET, "/api/v1/approvals?status=requested")
        .send()
        .await
        .context("list approvals failed")?;
    assert_eq!(approvals_response.status(), StatusCode::OK);
    let approvals_json = json_body(approvals_response).await?;
    let approval_id = approvals_json["items"]
        .as_array()
        .context("missing approval items")?
        .iter()
        .find(|item| item["run_id"] == run_id)
        .and_then(|item| item["approval_id"].as_str())
        .context("missing approval id for run")?
        .to_string();

    let resolved_approval = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/approvals/{approval_id}/resolve"),
            "op-1",
        )
        .json(&json!({
            "decision": "approve",
            "decided_via": "e2e"
        }))
        .send()
        .await
        .context("resolve approval request failed")?;
    assert_eq!(resolved_approval.status(), StatusCode::OK);

    let resumed_run = gateway
        .request(Method::POST, format!("/api/v1/runs/{run_id}/resume"))
        .send()
        .await
        .context("resume run request failed")?;
    assert_eq!(resumed_run.status(), StatusCode::OK);
    let resumed_json = json_body(resumed_run).await?;
    assert_eq!(resumed_json["run"]["status"], "succeeded");

    let timeline = gateway
        .request(
            Method::GET,
            format!("/api/v1/sessions/{session_id}/messages?limit=50"),
        )
        .send()
        .await
        .context("list messages request failed")?;
    assert_eq!(timeline.status(), StatusCode::OK);
    let timeline_json = json_body(timeline).await?;
    let timeline_items = timeline_json["items"]
        .as_array()
        .context("timeline items missing")?;
    assert_eq!(timeline_items.len(), 2);
    assert_eq!(timeline_items[0]["role"], "user");
    assert_eq!(timeline_items[1]["role"], "assistant");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn scheduler_executes_due_job_and_persists_history() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-scheduler", None).await?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let created_job = gateway
        .request(Method::POST, "/api/v1/jobs/add")
        .json(&json!({
            "agent_id": "default",
            "name": "scheduler-once-job",
            "enabled": true,
            "schedule_kind": "once",
            "run_at_ms": now_ms + 200,
            "payload_json": {
                "mode":"session.run",
                "session_key":"e2e:scheduler:session-run",
                "session_title":"e2e scheduler session",
                "input":"scheduled run from process test",
                "model_provider":"mock",
                "model_id":"mock-echo-v1"
            },
            "max_retries": 0,
            "retry_backoff_ms": 10,
            "timeout_ms": 500
        }))
        .send()
        .await
        .context("create job request failed")?;
    assert_eq!(created_job.status(), StatusCode::CREATED);
    let created_job_json = json_body(created_job).await?;
    let job_id = created_job_json["job"]["job_id"]
        .as_str()
        .context("missing job_id")?
        .to_string();

    let mut found = None;
    for _ in 0..60 {
        let history = gateway
            .request(
                Method::GET,
                format!("/api/v1/jobs/{job_id}/history?limit=5"),
            )
            .send()
            .await
            .context("job history poll failed")?;
        if history.status() != StatusCode::OK {
            anyhow::bail!("unexpected history status {}", history.status());
        }
        let history_json = json_body(history).await?;
        if let Some(first) = history_json["items"]
            .as_array()
            .and_then(|items| items.first())
        {
            found = Some(first.clone());
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    let first = found.context("scheduler did not create job run in time")?;
    assert_eq!(first["status"], "succeeded");
    assert_eq!(first["trigger_kind"], "scheduler");
    let output_json = first["output_json"]
        .as_str()
        .context("missing scheduler output_json")?;
    let output: serde_json::Value =
        serde_json::from_str(output_json).context("invalid scheduler output_json payload")?;
    assert_eq!(output["mode"], "session.run");
    if let Some(run_status) = output["run_status"].as_str() {
        assert_eq!(run_status, "succeeded");
    } else {
        anyhow::ensure!(
            output["run_id"].as_str().is_some(),
            "scheduler output payload is missing both run_status and run_id"
        );
    }
    assert!(output["session_id"].as_str().is_some());
    assert!(output["run_id"].as_str().is_some());

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_session_flows_remain_stable() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-concurrency", None).await?;
    let base = gateway.http_base();
    let token = gateway.token().to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("failed to create concurrency client")?;

    const TASKS: usize = 20;
    let tasks = (0..TASKS)
        .map(|idx| {
            let base = base.clone();
            let token = token.clone();
            let client = client.clone();
            tokio::spawn(async move {
                let created_session = client
                    .post(format!("{base}/api/v1/sessions"))
                    .bearer_auth(&token)
                    .json(&json!({ "title": format!("parallel-{idx}") }))
                    .send()
                    .await
                    .context("create session request failed")?;
                if created_session.status() != StatusCode::CREATED {
                    anyhow::bail!(
                        "unexpected create session status {}",
                        created_session.status()
                    );
                }
                let created_session_json: serde_json::Value = created_session
                    .json()
                    .await
                    .context("invalid create session response JSON")?;
                let session_id = created_session_json["session"]["session_id"]
                    .as_str()
                    .context("missing session_id")?
                    .to_string();

                let created_message = client
                    .post(format!("{base}/api/v1/sessions/{session_id}/messages"))
                    .bearer_auth(&token)
                    .json(&json!({
                        "role": "user",
                        "content_text": format!("parallel-message-{idx}")
                    }))
                    .send()
                    .await
                    .context("create message request failed")?;
                if created_message.status() != StatusCode::CREATED {
                    anyhow::bail!(
                        "unexpected create message status {}",
                        created_message.status()
                    );
                }

                let created_run = client
                    .post(format!("{base}/api/v1/sessions/{session_id}/runs"))
                    .bearer_auth(&token)
                    .json(&json!({}))
                    .send()
                    .await
                    .context("create run request failed")?;
                if created_run.status() != StatusCode::CREATED {
                    anyhow::bail!("unexpected create run status {}", created_run.status());
                }
                let run_json: serde_json::Value = created_run
                    .json()
                    .await
                    .context("invalid run response JSON")?;
                if run_json["run"]["status"] != "succeeded" {
                    anyhow::bail!("run did not succeed: {run_json}");
                }

                Ok::<String, anyhow::Error>(session_id)
            })
        })
        .collect::<Vec<_>>();

    let mut session_ids = Vec::with_capacity(TASKS);
    for join_result in join_all(tasks).await {
        let session_id = join_result.context("concurrency task panicked")??;
        session_ids.push(session_id);
    }

    for session_id in &session_ids {
        let timeline = gateway
            .request(
                Method::GET,
                format!("/api/v1/sessions/{session_id}/messages?limit=10"),
            )
            .send()
            .await
            .context("timeline request failed")?;
        assert_eq!(timeline.status(), StatusCode::OK);
        let timeline_json = json_body(timeline).await?;
        let items = timeline_json["items"]
            .as_array()
            .context("timeline items missing")?;
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["role"], "user");
        assert_eq!(items[1]["role"], "assistant");
    }

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn request_logs_are_written_to_state_log_directory() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-logs", None).await?;

    let health = gateway
        .request(Method::GET, "/api/v1/health")
        .send()
        .await
        .context("health request failed")?;
    assert_eq!(health.status(), StatusCode::OK);
    assert!(health.headers().get("x-request-id").is_some());

    drop(gateway);
    let logs_dir = state_dir.path().join("logs");
    let deadline = std::time::Instant::now() + Duration::from_secs(6);
    let mut found_non_empty_log = false;
    while std::time::Instant::now() < deadline {
        if let Ok(entries) = fs::read_dir(&logs_dir) {
            for entry in entries {
                let entry = entry.context("failed to read log dir entry")?;
                if !entry
                    .file_type()
                    .context("failed to read log file type")?
                    .is_file()
                {
                    continue;
                }
                let metadata = entry.metadata().with_context(|| {
                    format!("failed to read metadata for {}", entry.path().display())
                })?;
                if metadata.len() == 0 {
                    continue;
                }
                let content = fs::read_to_string(entry.path())
                    .with_context(|| format!("failed to read {}", entry.path().display()))?;
                let has_request_id = content.contains("request_id=")
                    || content.contains("\"request_id\"")
                    || content.contains("x-request-id");
                let has_health_request_context = content.contains("/api/v1/health")
                    || content.contains("uri=/api/v1/health")
                    || content.contains("\"uri\":\"/api/v1/health\"")
                    || content.contains("method=GET uri=/api/v1/health")
                    || content.contains("GET /api/v1/health")
                    || content.contains("http.request");
                let has_request_context = has_health_request_context
                    && (has_request_id
                        || content.contains("status=200")
                        || content.contains("status=OK"));
                if has_request_context {
                    found_non_empty_log = true;
                    break;
                }
            }
        }
        if found_non_empty_log {
            break;
        }
        sleep(Duration::from_millis(200)).await;
    }
    assert!(
        found_non_empty_log,
        "expected request logs with request_id to be written"
    );
    Ok(())
}

#[derive(Clone)]
struct NumquamStubState {
    resolve_calls: Arc<AtomicU64>,
}

struct NumquamStubServer {
    base_url: String,
    #[allow(dead_code)]
    resolve_calls: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl NumquamStubServer {
    async fn spawn() -> Result<Self> {
        let state = NumquamStubState {
            resolve_calls: Arc::new(AtomicU64::new(0)),
        };
        let app = Router::new()
            .route(
                "/api/integration/v1/context/build",
                post(numquam_stub_context_build),
            )
            .route(
                "/api/integration/v1/writeback/propose",
                post(numquam_stub_writeback_propose),
            )
            .route(
                "/api/integration/v1/writeback/resolve",
                post(numquam_stub_writeback_resolve),
            )
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind numquam stub listener failed")?;
        let addr = listener
            .local_addr()
            .context("read numquam stub local addr failed")?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(Self {
            base_url: format!("http://{addr}"),
            resolve_calls: state.resolve_calls,
            task,
        })
    }
}

impl Drop for NumquamStubServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn numquam_stub_context_build(
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let request_id = payload
        .get("request_id")
        .and_then(|value| value.as_str())
        .unwrap_or("req_stub_process_context");
    Json(json!({
        "schema_version": "integration.v1",
        "request_id": request_id,
        "request_id_source": "client",
        "operation": "context.build",
        "ok": true,
        "degrade_mode": false,
        "warnings": [],
        "data": {
            "context_text": "process-level stub memory context",
            "evidence": [{
                "evidence_id": "ev_proc_1",
                "section": "fact",
                "kind": "fact_card",
                "summary": "process memory evidence",
                "citations": ["session#1"],
                "confidence": 0.77
            }],
            "route": "ltm_light",
            "confidence": 0.77
        }
    }))
}

async fn numquam_stub_writeback_propose(
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let request_id = payload
        .get("request_id")
        .and_then(|value| value.as_str())
        .unwrap_or("req_stub_process_propose");
    let run_id = payload
        .get("run_id")
        .and_then(|value| value.as_str())
        .unwrap_or("run_stub");
    Json(json!({
        "schema_version": "integration.v1",
        "request_id": request_id,
        "request_id_source": "client",
        "operation": "writeback.propose",
        "ok": true,
        "degrade_mode": false,
        "warnings": [],
        "data": {
            "proposal_id": format!("proposal_{run_id}"),
            "status": "pending_review",
            "idempotent_replay": false,
            "audit_ref": format!("audit_{run_id}")
        }
    }))
}

async fn numquam_stub_writeback_resolve(
    AxumState(state): AxumState<NumquamStubState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    state.resolve_calls.fetch_add(1, Ordering::Relaxed);
    let request_id = payload
        .get("request_id")
        .and_then(|value| value.as_str())
        .unwrap_or("req_stub_process_resolve");
    let proposal_id = payload
        .get("data")
        .and_then(|value| value.get("proposal_id"))
        .and_then(|value| value.as_str())
        .unwrap_or("proposal_stub");
    let decision = payload
        .get("data")
        .and_then(|value| value.get("decision"))
        .and_then(|value| value.as_str())
        .unwrap_or("approve");
    let status = if decision == "approve" {
        "approved"
    } else {
        "rejected"
    };
    Json(json!({
        "schema_version": "integration.v1",
        "request_id": request_id,
        "request_id_source": "client",
        "operation": "writeback.resolve",
        "ok": true,
        "degrade_mode": false,
        "warnings": [],
        "data": {
            "proposal_id": proposal_id,
            "status": status,
            "already_resolved": false,
            "resolved_at_utc": "2026-02-19T00:00:00Z",
            "audit_ref": "audit_resolve_stub"
        }
    }))
}

#[derive(Default)]
struct ObservedEvents {
    run_created: bool,
    run_status_running: bool,
    run_status_succeeded: bool,
    run_delta: bool,
    approval_requested: bool,
    approval_resolved: bool,
}

impl ObservedEvents {
    fn observe(&mut self, frame: &serde_json::Value) {
        let event = match frame["event"].as_str() {
            Some(event) => event,
            None => return,
        };

        match event {
            "run.created" => self.run_created = true,
            "run.status" => match frame["data"]["status"].as_str() {
                Some("running") => self.run_status_running = true,
                Some("succeeded") => self.run_status_succeeded = true,
                _ => {}
            },
            "run.delta" => self.run_delta = true,
            "approval.requested" => self.approval_requested = true,
            "approval.resolved" => self.approval_resolved = true,
            _ => {}
        }
    }

    fn is_complete(&self) -> bool {
        self.run_created
            && self.run_status_running
            && self.run_status_succeeded
            && self.run_delta
            && self.approval_requested
            && self.approval_resolved
    }
}

async fn next_ws_event(ws: &mut WsStream) -> Result<serde_json::Value> {
    loop {
        let message = ws
            .next()
            .await
            .ok_or_else(|| anyhow!("websocket stream closed"))??;
        match message {
            Message::Text(text) => {
                let frame: serde_json::Value =
                    serde_json::from_str(&text).context("invalid websocket JSON event frame")?;
                return Ok(frame);
            }
            Message::Binary(bytes) => {
                let frame: serde_json::Value = serde_json::from_slice(&bytes)
                    .context("invalid binary websocket JSON event frame")?;
                return Ok(frame);
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => return Err(anyhow!("websocket closed by server")),
            Message::Frame(_) => continue,
        }
    }
}

async fn wait_for_ws_event(
    ws: &mut WsStream,
    expected_event: &str,
    max_wait: Duration,
) -> Result<serde_json::Value> {
    let deadline = tokio::time::Instant::now() + max_wait;
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(anyhow!(
                "timed out waiting for websocket event '{}'",
                expected_event
            ));
        }
        let remaining = deadline - now;
        let frame = timeout(remaining, next_ws_event(ws))
            .await
            .context("timed out waiting for websocket frame")??;
        if frame["event"].as_str() == Some(expected_event) {
            return Ok(frame);
        }
    }
}
