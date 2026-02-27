mod common;

use anyhow::{anyhow, Context, Result};
use axum::extract::State as AxumState;
use axum::routing::post;
use axum::{Json, Router};
use common::{json_body, GatewayProcess, WsStream};
use futures_util::future::join_all;
use futures_util::StreamExt;
use httpmock::MockServer;
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

    let gateway_status =
        wait_for_ws_event(&mut ws, "gateway.status", Duration::from_secs(2)).await?;
    assert_eq!(gateway_status["schema_version"], "carsinos.ws.event.v1");
    assert_eq!(gateway_status["event_type"], "gateway.status");
    assert_eq!(gateway_status["entity"], "gateway");
    assert_eq!(gateway_status["payload"]["status"], "ok");
    assert!(gateway_status["event_id"].as_str().is_some());

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
async fn websocket_accepts_query_token_auth() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway =
        GatewayProcess::spawn(state_dir.path(), "e2e-token-ws-query", Some("op-1")).await?;
    let mut ws = gateway.connect_ws_with_query_token().await?;

    let gateway_status =
        wait_for_ws_event(&mut ws, "gateway.status", Duration::from_secs(2)).await?;
    assert_eq!(gateway_status["schema_version"], "carsinos.ws.event.v1");
    assert_eq!(gateway_status["event_type"], "gateway.status");
    assert_eq!(gateway_status["payload"]["status"], "ok");
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
    let mut reached_terminal_state = false;
    for _ in 0..80 {
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
            if first["status"] != "running" && first["status"] != "queued" {
                reached_terminal_state = true;
                break;
            }
        }
        sleep(Duration::from_millis(100)).await;
    }

    let first = found.context("scheduler did not create job run in time")?;
    anyhow::ensure!(
        reached_terminal_state,
        "scheduler job did not reach terminal state in time"
    );
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
async fn scheduler_marks_run_failed_when_payload_exceeds_timeout() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-scheduler-timeout", None)
        .await
        .context("failed to start scheduler-timeout process")?;
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    let created_job = gateway
        .request(Method::POST, "/api/v1/jobs/add")
        .json(&json!({
            "agent_id": "default",
            "name": "scheduler-timeout-job",
            "enabled": true,
            "schedule_kind": "once",
            "run_at_ms": now_ms + 200,
            "payload_json": {
                "mode":"noop",
                "delay_ms": 120
            },
            "max_retries": 0,
            "retry_backoff_ms": 10,
            "timeout_ms": 50
        }))
        .send()
        .await
        .context("create scheduler-timeout job failed")?;
    assert_eq!(created_job.status(), StatusCode::CREATED);
    let created_job_json = json_body(created_job).await?;
    let job_id = created_job_json["job"]["job_id"]
        .as_str()
        .context("missing scheduler-timeout job_id")?
        .to_string();

    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut found = None;
    while std::time::Instant::now() < deadline {
        let history = gateway
            .request(
                Method::GET,
                format!("/api/v1/jobs/{job_id}/history?limit=5"),
            )
            .send()
            .await
            .context("poll scheduler-timeout history failed")?;
        if history.status() == StatusCode::OK {
            let history_json = json_body(history).await?;
            if let Some(first) = history_json["items"]
                .as_array()
                .and_then(|items| items.first())
            {
                if first["status"] != "running" && first["status"] != "queued" {
                    found = Some(first.clone());
                    break;
                }
            }
        }
        sleep(Duration::from_millis(120)).await;
    }

    let run = found.context("scheduler-timeout job did not reach terminal state")?;
    assert_eq!(run["status"], "failed");
    assert_eq!(run["trigger_kind"], "scheduler");
    assert!(
        run["error_text"]
            .as_str()
            .unwrap_or_default()
            .starts_with("TIMEOUT:"),
        "expected timeout error text, got {}",
        run["error_text"]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn second_process_disables_scheduler_when_instance_lock_is_held() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let primary = GatewayProcess::spawn(state_dir.path(), "e2e-token-scheduler-primary", None)
        .await
        .context("failed to start primary process")?;
    let secondary = GatewayProcess::spawn(state_dir.path(), "e2e-token-scheduler-secondary", None)
        .await
        .context("failed to start secondary process")?;

    let primary_status = primary
        .request(Method::GET, "/api/v1/jobs/status")
        .send()
        .await
        .context("primary job status failed")?;
    assert_eq!(primary_status.status(), StatusCode::OK);
    let primary_status_json = json_body(primary_status).await?;
    assert_eq!(primary_status_json["scheduler_running"], true);

    let secondary_status = secondary
        .request(Method::GET, "/api/v1/jobs/status")
        .send()
        .await
        .context("secondary job status failed")?;
    assert_eq!(secondary_status.status(), StatusCode::OK);
    let secondary_status_json = json_body(secondary_status).await?;
    assert_eq!(secondary_status_json["scheduler_running"], false);

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let created_job = secondary
        .request(Method::POST, "/api/v1/jobs/add")
        .json(&json!({
            "agent_id": "default",
            "name": "scheduler-lock-process-job",
            "enabled": true,
            "schedule_kind": "once",
            "run_at_ms": now_ms + 200,
            "payload_json": {
                "mode":"noop",
                "message":"scheduler lock test"
            },
            "max_retries": 0,
            "retry_backoff_ms": 10,
            "timeout_ms": 500
        }))
        .send()
        .await
        .context("create lock test job failed")?;
    assert_eq!(created_job.status(), StatusCode::CREATED);
    let created_job_json = json_body(created_job).await?;
    let job_id = created_job_json["job"]["job_id"]
        .as_str()
        .context("missing lock-test job_id")?
        .to_string();

    let deadline = std::time::Instant::now() + Duration::from_secs(12);
    let mut history_item = None;
    while std::time::Instant::now() < deadline {
        let history = primary
            .request(
                Method::GET,
                format!("/api/v1/jobs/{job_id}/history?limit=5"),
            )
            .send()
            .await
            .context("poll lock-test history failed")?;
        if history.status() == StatusCode::OK {
            let history_json = json_body(history).await?;
            if let Some(first) = history_json["items"]
                .as_array()
                .and_then(|items| items.first())
            {
                if first["status"] != "running" && first["status"] != "queued" {
                    history_item = Some(first.clone());
                    break;
                }
            }
        }
        sleep(Duration::from_millis(120)).await;
    }

    let run = history_item.context("scheduler did not execute lock-test job in time")?;
    assert_eq!(run["status"], "succeeded");
    assert_eq!(run["trigger_kind"], "scheduler");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn run_is_stopped_by_wall_time_budget_guardrail() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn_with_env(
        state_dir.path(),
        "e2e-token-budget-guardrail",
        None,
        &[("CARSINOS_AUTO_APPROVE_TOOLS", "1")],
    )
    .await
    .context("failed to start budget-guardrail process")?;

    let config_update = gateway
        .request(Method::POST, "/api/v1/config/runtime")
        .json(&json!({
            "autonomy_guardrails": {
                "max_run_ms": 1000,
                "max_tool_calls_per_run": 16,
                "max_provider_input_chars": 32000,
                "max_tool_output_chars_total": 64000,
                "max_provider_attempts": 3,
                "max_consecutive_failures_before_breaker": 3,
                "heartbeat_max_run_ms": 500
            }
        }))
        .send()
        .await
        .context("runtime config update failed")?;
    assert_eq!(config_update.status(), StatusCode::OK);

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "budget-stop" }))
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
            "content_text": "tool.exec sleep 2"
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
    assert_eq!(run_json["run"]["status"], "failed");
    assert!(
        run_json["run"]["error_text"]
            .as_str()
            .unwrap_or_default()
            .contains("BUDGET_MAX_RUN_MS"),
        "expected budget stop reason in error text, got {}",
        run_json["run"]["error_text"]
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heartbeat_run_mode_rejects_tool_lines_process_level() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-heartbeat-guardrail", None)
        .await
        .context("failed to start heartbeat process")?;

    let create_job = gateway
        .request(Method::POST, "/api/v1/jobs/add")
        .json(&json!({
            "agent_id":"default",
            "name":"heartbeat-tool-reject",
            "enabled":true,
            "schedule_kind":"interval",
            "interval_seconds":60,
            "payload_json":{"mode":"heartbeat.run","input":"TOOL: fs.read {\"path\":\"/tmp/x\"}"},
            "max_retries":5,
            "retry_backoff_ms":5,
            "timeout_ms":1000
        }))
        .send()
        .await
        .context("create heartbeat reject job failed")?;
    assert_eq!(create_job.status(), StatusCode::CREATED);
    let create_job_json = json_body(create_job).await?;
    let job_id = create_job_json["job"]["job_id"]
        .as_str()
        .context("missing heartbeat reject job_id")?
        .to_string();

    let run_now = gateway
        .request(Method::POST, format!("/api/v1/jobs/{job_id}/run"))
        .send()
        .await
        .context("run heartbeat reject job failed")?;
    assert_eq!(run_now.status(), StatusCode::OK);
    let run_now_json = json_body(run_now).await?;
    assert_eq!(run_now_json["job_run"]["status"], "failed");
    assert_eq!(run_now_json["job_run"]["attempt"], 1);
    assert!(run_now_json["job_run"]["error_text"]
        .as_str()
        .unwrap_or_default()
        .contains("HEARTBEAT_TOOLS_FORBIDDEN"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tool_fanout_cap_and_repeated_tool_error_breaker_are_enforced_process_level() -> Result<()>
{
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn_with_env(
        state_dir.path(),
        "e2e-token-tool-breakers",
        None,
        &[("CARSINOS_AUTO_APPROVE_TOOLS", "1")],
    )
    .await
    .context("failed to start tool-breaker process")?;

    let guardrail_update = gateway
        .request(Method::POST, "/api/v1/config/runtime")
        .json(&json!({
            "autonomy_guardrails": {
                "max_consecutive_failures_before_breaker": 2
            }
        }))
        .send()
        .await
        .context("runtime guardrail update failed")?;
    assert_eq!(guardrail_update.status(), StatusCode::OK);

    let fanout_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "fanout-process" }))
        .send()
        .await
        .context("create fanout session failed")?;
    assert_eq!(fanout_session.status(), StatusCode::CREATED);
    let fanout_session_json = json_body(fanout_session).await?;
    let fanout_session_id = fanout_session_json["session"]["session_id"]
        .as_str()
        .context("missing fanout session_id")?
        .to_string();
    let fanout_input = (1..=9)
        .map(|idx| format!("tool.process status p{idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    let fanout_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{fanout_session_id}/messages"),
        )
        .json(&json!({ "role": "user", "content_text": fanout_input }))
        .send()
        .await
        .context("create fanout message failed")?;
    assert_eq!(fanout_message.status(), StatusCode::CREATED);
    let fanout_run = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{fanout_session_id}/runs"),
        )
        .json(&json!({}))
        .send()
        .await
        .context("create fanout run failed")?;
    assert_eq!(fanout_run.status(), StatusCode::CREATED);
    let fanout_run_json = json_body(fanout_run).await?;
    assert_eq!(fanout_run_json["run"]["status"], "failed");
    assert!(fanout_run_json["run"]["error_text"]
        .as_str()
        .unwrap_or_default()
        .contains("BREAKER_TOOL_FANOUT_CAP"));

    let repeated_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "repeated-breaker-process" }))
        .send()
        .await
        .context("create repeated-breaker session failed")?;
    assert_eq!(repeated_session.status(), StatusCode::CREATED);
    let repeated_session_json = json_body(repeated_session).await?;
    let repeated_session_id = repeated_session_json["session"]["session_id"]
        .as_str()
        .context("missing repeated-breaker session_id")?
        .to_string();
    let repeated_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{repeated_session_id}/messages"),
        )
        .json(&json!({ "role": "user", "content_text": "tool.process unknown 123" }))
        .send()
        .await
        .context("create repeated-breaker message failed")?;
    assert_eq!(repeated_message.status(), StatusCode::CREATED);

    let first_run = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{repeated_session_id}/runs"),
        )
        .json(&json!({}))
        .send()
        .await
        .context("create first repeated-breaker run failed")?;
    assert_eq!(first_run.status(), StatusCode::CREATED);
    let first_run_json = json_body(first_run).await?;
    assert_eq!(first_run_json["run"]["status"], "failed");
    assert!(!first_run_json["run"]["error_text"]
        .as_str()
        .unwrap_or_default()
        .contains("BREAKER_REPEATED_TOOL_ERROR"));

    let second_run = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{repeated_session_id}/runs"),
        )
        .json(&json!({}))
        .send()
        .await
        .context("create second repeated-breaker run failed")?;
    assert_eq!(second_run.status(), StatusCode::CREATED);
    let second_run_json = json_body(second_run).await?;
    assert_eq!(second_run_json["run"]["status"], "failed");
    assert!(second_run_json["run"]["error_text"]
        .as_str()
        .unwrap_or_default()
        .contains("BREAKER_REPEATED_TOOL_ERROR"));

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn daily_budget_kill_switch_is_enforced_process_level() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "e2e-token-budget-killswitch", None)
        .await
        .context("failed to start budget-killswitch process")?;

    let provider_policy_update = gateway
        .request(Method::POST, "/api/v1/config/runtime")
        .json(&json!({
            "providers": [
                {
                    "provider":"openai",
                    "enabled":true,
                    "allow_consumer_oauth":false,
                    "kill_switch_scope":"none",
                    "daily_token_budget":10
                },
                {
                    "provider":"anthropic",
                    "enabled":true,
                    "allow_consumer_oauth":false,
                    "kill_switch_scope":"none"
                }
            ]
        }))
        .send()
        .await
        .context("provider policy update failed")?;
    assert_eq!(provider_policy_update.status(), StatusCode::OK);

    let server = MockServer::start_async().await;
    let _completion = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions");
            then.status(200)
                .header("content-type", "application/json")
                .body(
                    r#"{
                        "choices":[{"message":{"content":"budget process test"}}],
                        "usage":{"prompt_tokens":8,"completion_tokens":4,"total_tokens":12}
                    }"#,
                );
        })
        .await;
    let api_base_url = server.url("");

    let create_profile = gateway
        .request(Method::POST, "/api/v1/auth/profiles")
        .json(&json!({
            "provider":"openai",
            "display_name":"process-budget-profile",
            "auth_mode":"api_key",
            "risk_level":"low",
            "enabled":true,
            "kill_switch_scope":"none",
            "api_base_url": api_base_url,
            "credentials_json":{"api_key":"budget-process-key"}
        }))
        .send()
        .await
        .context("create process budget profile failed")?;
    assert_eq!(create_profile.status(), StatusCode::CREATED);
    let create_profile_json = json_body(create_profile).await?;
    let profile_id = create_profile_json["profile"]["auth_profile_id"]
        .as_str()
        .context("missing process budget profile_id")?
        .to_string();

    let session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "budget-killswitch-process" }))
        .send()
        .await
        .context("create budget-killswitch session failed")?;
    assert_eq!(session.status(), StatusCode::CREATED);
    let session_json = json_body(session).await?;
    let session_id = session_json["session"]["session_id"]
        .as_str()
        .context("missing budget-killswitch session_id")?
        .to_string();
    let message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({ "role": "user", "content_text": "budget enforcement process level" }))
        .send()
        .await
        .context("create budget-killswitch message failed")?;
    assert_eq!(message.status(), StatusCode::CREATED);

    let run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({
            "model_provider":"openai",
            "model_id":"gpt-test",
            "auth_profile_id": profile_id.clone()
        }))
        .send()
        .await
        .context("create budget-killswitch run failed")?;
    assert_eq!(run.status(), StatusCode::CREATED);
    let run_json = json_body(run).await?;
    assert_eq!(run_json["run"]["status"], "failed");
    assert!(run_json["run"]["error_text"]
        .as_str()
        .unwrap_or_default()
        .contains("BUDGET_DAILY_TOKEN_LIMIT"));

    let profiles = gateway
        .request(
            Method::GET,
            "/api/v1/auth/profiles?provider=openai&include_disabled=true",
        )
        .send()
        .await
        .context("list auth profiles after budget breach failed")?;
    assert_eq!(profiles.status(), StatusCode::OK);
    let profiles_json = json_body(profiles).await?;
    let profile = profiles_json["items"]
        .as_array()
        .context("profiles array missing")?
        .iter()
        .find(|item| item["auth_profile_id"] == profile_id)
        .context("updated budget profile not found")?;
    assert_eq!(profile["enabled"], false);
    assert_eq!(profile["kill_switch_scope"], "profile");

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
async fn parallel_runs_for_same_session_return_conflict() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn_with_env(
        state_dir.path(),
        "e2e-token-lane-lock",
        None,
        &[("CARSINOS_AUTO_APPROVE_TOOLS", "1")],
    )
    .await?;

    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": "lane-lock-process" }))
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
            "content_text": "tool.exec sleep 1"
        }))
        .send()
        .await
        .context("create message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let run_url = format!("{}/api/v1/sessions/{session_id}/runs", gateway.http_base());
    let token = gateway.token().to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("failed to build run concurrency client")?;

    let left = client
        .post(&run_url)
        .bearer_auth(&token)
        .json(&json!({}))
        .send();
    let right = client
        .post(&run_url)
        .bearer_auth(&token)
        .json(&json!({}))
        .send();
    let (left_response, right_response) = tokio::join!(left, right);
    let left_status = left_response.context("left run request failed")?.status();
    let right_status = right_response.context("right run request failed")?.status();

    assert!(
        (left_status == StatusCode::CREATED && right_status == StatusCode::CONFLICT)
            || (left_status == StatusCode::CONFLICT && right_status == StatusCode::CREATED),
        "unexpected statuses: left={left_status} right={right_status}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agent_mail_flow_supports_threads_messages_attachments_and_leases() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn_with_env(
        state_dir.path(),
        "e2e-token-agent-mail",
        Some("op-1"),
        &[
            ("CARSINOS_AGENT_MAIL_ATTACHMENT_MAX_BYTES", "4096"),
            (
                "CARSINOS_AGENT_MAIL_ALLOWED_MIME",
                "text/plain,application/json",
            ),
        ],
    )
    .await?;

    let created_thread = gateway
        .request_with_operator(Method::POST, "/api/v1/agent-mail/threads", "op-1")
        .json(&json!({
            "kind": "direct",
            "subject": "lyra-claude-sync",
            "participants": ["lyra", "claude"]
        }))
        .send()
        .await
        .context("create agent-mail thread request failed")?;
    assert_eq!(created_thread.status(), StatusCode::CREATED);
    let created_thread_json = json_body(created_thread).await?;
    let thread_id = created_thread_json["thread"]["thread_id"]
        .as_str()
        .context("missing agent-mail thread_id")?
        .to_string();

    let listed_threads = gateway
        .request_with_operator(
            Method::GET,
            "/api/v1/agent-mail/threads?kind=direct&mailbox=all&principal_id=claude&limit=30",
            "op-1",
        )
        .send()
        .await
        .context("list agent-mail threads request failed")?;
    assert_eq!(listed_threads.status(), StatusCode::OK);
    let listed_threads_json = json_body(listed_threads).await?;
    let listed_items = listed_threads_json["items"]
        .as_array()
        .context("agent-mail list items missing")?;
    assert!(listed_items
        .iter()
        .any(|item| item["thread_id"] == thread_id));

    let sent_message = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/agent-mail/threads/{thread_id}/messages"),
            "op-1",
        )
        .json(&json!({
            "sender_principal": "lyra",
            "sender_kind": "agent",
            "body_text": "handoff payload: sync task state",
            "recipients": ["claude"],
            "metadata_json": {
                "mode": "handoff",
                "priority": "normal"
            }
        }))
        .send()
        .await
        .context("send agent-mail message request failed")?;
    assert_eq!(sent_message.status(), StatusCode::CREATED);
    let sent_message_json = json_body(sent_message).await?;
    let message_id = sent_message_json["message"]["message_id"]
        .as_str()
        .context("missing message_id")?
        .to_string();

    let listed_messages = gateway
        .request_with_operator(
            Method::GET,
            format!("/api/v1/agent-mail/threads/{thread_id}/messages?limit=100"),
            "op-1",
        )
        .send()
        .await
        .context("list agent-mail messages request failed")?;
    assert_eq!(listed_messages.status(), StatusCode::OK);
    let listed_messages_json = json_body(listed_messages).await?;
    let message_items = listed_messages_json["items"]
        .as_array()
        .context("message items missing")?;
    let first_message = message_items.first().context("no message returned")?;
    assert_eq!(first_message["message_id"], message_id);
    assert_eq!(first_message["sender_principal"], "lyra");

    let acknowledged = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/agent-mail/messages/{message_id}/ack"),
            "op-1",
        )
        .json(&json!({
            "recipient_principal": "claude"
        }))
        .send()
        .await
        .context("ack agent-mail message request failed")?;
    assert_eq!(acknowledged.status(), StatusCode::OK);
    let acknowledged_json = json_body(acknowledged).await?;
    assert_eq!(acknowledged_json["recipient_principal"], "claude");
    assert!(acknowledged_json["acked_at"].as_i64().is_some());

    let uploaded_attachment = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/agent-mail/messages/{message_id}/attachments/upload"),
            "op-1",
        )
        .json(&json!({
            "filename": "handoff.txt",
            "mime": "text/plain",
            "content_base64": "aGVsbG8gd29ybGQ="
        }))
        .send()
        .await
        .context("upload agent-mail attachment request failed")?;
    assert_eq!(uploaded_attachment.status(), StatusCode::OK);
    let uploaded_attachment_json = json_body(uploaded_attachment).await?;
    let attachment_id = uploaded_attachment_json["attachment"]["attachment_id"]
        .as_str()
        .context("missing attachment_id")?
        .to_string();

    let downloaded_attachment = gateway
        .request_with_operator(
            Method::GET,
            format!("/api/v1/agent-mail/messages/{message_id}/attachments/{attachment_id}"),
            "op-1",
        )
        .send()
        .await
        .context("download agent-mail attachment request failed")?;
    assert_eq!(downloaded_attachment.status(), StatusCode::OK);
    let content_type = downloaded_attachment
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .context("content-type header missing on attachment download response")?;
    assert!(content_type.starts_with("text/plain"));
    let downloaded_bytes = downloaded_attachment
        .bytes()
        .await
        .context("reading downloaded attachment bytes failed")?;
    assert_eq!(downloaded_bytes.as_ref(), b"hello world");

    let created_lease = gateway
        .request_with_operator(Method::POST, "/api/v1/agent-mail/leases", "op-1")
        .json(&json!({
            "holder_principal": "lyra",
            "glob_pattern": "workspace/**",
            "exclusive": true,
            "ttl_ms": 600000,
            "note": "serialize edits"
        }))
        .send()
        .await
        .context("create agent-mail lease request failed")?;
    assert_eq!(created_lease.status(), StatusCode::CREATED);
    let created_lease_json = json_body(created_lease).await?;
    let lease_id = created_lease_json["lease"]["lease_id"]
        .as_str()
        .context("missing lease_id")?
        .to_string();

    let conflicting_lease = gateway
        .request_with_operator(Method::POST, "/api/v1/agent-mail/leases", "op-1")
        .json(&json!({
            "holder_principal": "claude",
            "glob_pattern": "workspace/**",
            "exclusive": true,
            "ttl_ms": 600000,
            "note": "conflict expected"
        }))
        .send()
        .await
        .context("conflicting lease request failed")?;
    assert_eq!(conflicting_lease.status(), StatusCode::CONFLICT);

    let released_lease = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/agent-mail/leases/{lease_id}/release"),
            "op-1",
        )
        .json(&json!({
            "holder_principal": "lyra"
        }))
        .send()
        .await
        .context("release lease request failed")?;
    assert_eq!(released_lease.status(), StatusCode::OK);
    let released_lease_json = json_body(released_lease).await?;
    assert!(released_lease_json["lease"]["released_at"]
        .as_i64()
        .is_some());

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
    let second_health = gateway
        .request(Method::GET, "/api/v1/health")
        .send()
        .await
        .context("second health request failed")?;
    assert_eq!(second_health.status(), StatusCode::OK);

    let logs_dir = state_dir.path().join("logs");
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    let mut found_request_log_marker = false;
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
                let has_request_id_marker = content.contains("request_id=")
                    || content.contains("\"request_id\"")
                    || content.contains("x-request-id")
                    || content.contains("request_id:");
                let has_http_marker = content.contains("http.request")
                    || content.contains("status=200")
                    || content.contains("method=GET")
                    || content.contains("/api/v1/health");
                let has_tracing_init_marker = content.contains("tracing initialized");
                if (has_request_id_marker && has_http_marker) || has_tracing_init_marker {
                    found_request_log_marker = true;
                    break;
                }
            }
        }
        if found_request_log_marker {
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }
    drop(gateway);
    assert!(
        found_request_log_marker,
        "expected request-log markers to be written to state log directory"
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
