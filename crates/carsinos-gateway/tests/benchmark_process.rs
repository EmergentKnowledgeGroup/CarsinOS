mod common;

use anyhow::{Context, Result};
use axum::extract::State as AxumState;
use axum::routing::post;
use axum::{Json, Router};
use common::{json_body, GatewayProcess};
use futures_util::future::join_all;
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use tokio::net::TcpListener;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_gateway_end_to_end_latency() -> Result<()> {
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn(state_dir.path(), "bench-token", Some("bench-op")).await?;

    const HEALTH_WARMUP: usize = 20;
    const HEALTH_SAMPLES: usize = 180;
    for _ in 0..HEALTH_WARMUP {
        let response = gateway
            .request(Method::GET, "/api/v1/health")
            .send()
            .await
            .context("health warmup request failed")?;
        assert_eq!(response.status(), StatusCode::OK);
    }

    let mut health_latencies = Vec::with_capacity(HEALTH_SAMPLES);
    for _ in 0..HEALTH_SAMPLES {
        let started = Instant::now();
        let response = gateway
            .request(Method::GET, "/api/v1/health")
            .send()
            .await
            .context("health benchmark request failed")?;
        assert_eq!(response.status(), StatusCode::OK);
        health_latencies.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    const FLOW_WARMUP: usize = 6;
    const FLOW_SAMPLES: usize = 50;
    for i in 0..FLOW_WARMUP {
        let title = format!("bench-warmup-{i}");
        run_flow_iteration(&gateway, &title).await?;
    }

    let mut flow_latencies = Vec::with_capacity(FLOW_SAMPLES);
    for i in 0..FLOW_SAMPLES {
        let title = format!("bench-sample-{i}");
        let started = Instant::now();
        run_flow_iteration(&gateway, &title).await?;
        flow_latencies.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    const APPROVAL_WARMUP: usize = 4;
    const APPROVAL_SAMPLES: usize = 35;
    for i in 0..APPROVAL_WARMUP {
        let title = format!("approval-warmup-{i}");
        run_approval_iteration(&gateway, &title).await?;
    }

    let mut approval_latencies = Vec::with_capacity(APPROVAL_SAMPLES);
    for i in 0..APPROVAL_SAMPLES {
        let title = format!("approval-sample-{i}");
        let started = Instant::now();
        run_approval_iteration(&gateway, &title).await?;
        approval_latencies.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let health_stats = LatencyStats::from_samples(&mut health_latencies)
        .context("failed to summarize health latency samples")?;
    let flow_stats = LatencyStats::from_samples(&mut flow_latencies)
        .context("failed to summarize flow latency samples")?;
    let approval_stats = LatencyStats::from_samples(&mut approval_latencies)
        .context("failed to summarize approval latency samples")?;
    let burst_stats = run_concurrent_health_burst(&gateway, 240, 40).await?;

    eprintln!(
        "BENCH health(ms): p50={:.2} p95={:.2} p99={:.2} avg={:.2} max={:.2}",
        health_stats.p50_ms,
        health_stats.p95_ms,
        health_stats.p99_ms,
        health_stats.avg_ms,
        health_stats.max_ms
    );
    eprintln!(
        "BENCH flow(ms): p50={:.2} p95={:.2} p99={:.2} avg={:.2} max={:.2}",
        flow_stats.p50_ms,
        flow_stats.p95_ms,
        flow_stats.p99_ms,
        flow_stats.avg_ms,
        flow_stats.max_ms
    );
    eprintln!(
        "BENCH approval-flow(ms): p50={:.2} p95={:.2} p99={:.2} avg={:.2} max={:.2}",
        approval_stats.p50_ms,
        approval_stats.p95_ms,
        approval_stats.p99_ms,
        approval_stats.avg_ms,
        approval_stats.max_ms
    );
    eprintln!(
        "BENCH health-burst: requests={} concurrency={} throughput_rps={:.2} p95_ms={:.2} p99_ms={:.2} avg_ms={:.2} max_ms={:.2}",
        burst_stats.total_requests,
        burst_stats.concurrency,
        burst_stats.throughput_rps,
        burst_stats.p95_ms,
        burst_stats.p99_ms,
        burst_stats.avg_ms,
        burst_stats.max_ms
    );

    // Thresholds are strict enough to detect meaningful regressions while staying stable on CI-class machines.
    assert!(
        health_stats.p95_ms < 120.0,
        "health endpoint p95 too slow: {:.2}ms",
        health_stats.p95_ms
    );
    assert!(
        health_stats.p99_ms < 200.0,
        "health endpoint p99 too slow: {:.2}ms",
        health_stats.p99_ms
    );
    assert!(
        flow_stats.p95_ms < 1_100.0,
        "session/message/run flow p95 too slow: {:.2}ms",
        flow_stats.p95_ms
    );
    assert!(
        flow_stats.p99_ms < 2_000.0,
        "session/message/run flow p99 too slow: {:.2}ms",
        flow_stats.p99_ms
    );
    assert!(
        approval_stats.p95_ms < 1_400.0,
        "approval flow p95 too slow: {:.2}ms",
        approval_stats.p95_ms
    );
    assert!(
        approval_stats.p99_ms < 2_200.0,
        "approval flow p99 too slow: {:.2}ms",
        approval_stats.p99_ms
    );
    assert!(
        burst_stats.throughput_rps > 120.0,
        "health burst throughput too low: {:.2} rps",
        burst_stats.throughput_rps
    );
    assert!(
        burst_stats.p95_ms < 220.0,
        "health burst p95 too slow: {:.2}ms",
        burst_stats.p95_ms
    );
    assert!(
        burst_stats.p99_ms < 300.0,
        "health burst p99 too slow: {:.2}ms",
        burst_stats.p99_ms
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn benchmark_numquam_integrated_flow_latency() -> Result<()> {
    let numquam_stub = NumquamBenchmarkStub::spawn().await?;
    let state_dir = TempDir::new().context("failed to create temp state directory")?;
    let gateway = GatewayProcess::spawn_with_env(
        state_dir.path(),
        "bench-token-numquam",
        Some("bench-op"),
        &[
            ("CARSINOS_NUMQUAM_ENABLED", "1"),
            ("CARSINOS_NUMQUAM_TRANSPORT", "http"),
            ("CARSINOS_NUMQUAM_BASE_URL", numquam_stub.base_url.as_str()),
            ("CARSINOS_NUMQUAM_TOKEN", "stub-token"),
            ("CARSINOS_NUMQUAM_TIMEOUT_MS", "3000"),
        ],
    )
    .await?;

    const WARMUP: usize = 8;
    const SAMPLES: usize = 45;
    for i in 0..WARMUP {
        let title = format!("numquam-warmup-{i}");
        run_flow_iteration(&gateway, &title).await?;
    }

    let mut latencies = Vec::with_capacity(SAMPLES);
    for i in 0..SAMPLES {
        let title = format!("numquam-sample-{i}");
        let started = Instant::now();
        run_flow_iteration(&gateway, &title).await?;
        latencies.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    let stats = LatencyStats::from_samples(&mut latencies)
        .context("failed to summarize numquam integrated latency samples")?;
    eprintln!(
        "BENCH numquam-flow(ms): p50={:.2} p95={:.2} p99={:.2} avg={:.2} max={:.2} resolve_calls={}",
        stats.p50_ms,
        stats.p95_ms,
        stats.p99_ms,
        stats.avg_ms,
        stats.max_ms,
        numquam_stub.resolve_calls.load(Ordering::Relaxed)
    );

    assert!(
        stats.p95_ms < 1_400.0,
        "numquam integrated flow p95 too slow: {:.2}ms",
        stats.p95_ms
    );
    assert!(
        stats.p99_ms < 2_200.0,
        "numquam integrated flow p99 too slow: {:.2}ms",
        stats.p99_ms
    );
    Ok(())
}

async fn run_flow_iteration(gateway: &GatewayProcess, title: &str) -> Result<()> {
    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": title }))
        .send()
        .await
        .context("create session benchmark request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id in benchmark create_session response")?;

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "benchmark content"
        }))
        .send()
        .await
        .context("create message benchmark request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({}))
        .send()
        .await
        .context("create run benchmark request failed")?;
    assert_eq!(created_run.status(), StatusCode::CREATED);
    let created_run_json = json_body(created_run).await?;
    assert_eq!(created_run_json["run"]["status"], "succeeded");

    Ok(())
}

async fn run_approval_iteration(gateway: &GatewayProcess, title: &str) -> Result<()> {
    let created_session = gateway
        .request(Method::POST, "/api/v1/sessions")
        .json(&json!({ "title": title }))
        .send()
        .await
        .context("create approval session benchmark request failed")?;
    assert_eq!(created_session.status(), StatusCode::CREATED);
    let created_session_json = json_body(created_session).await?;
    let session_id = created_session_json["session"]["session_id"]
        .as_str()
        .context("missing session_id in approval benchmark create_session response")?;

    let created_message = gateway
        .request(
            Method::POST,
            format!("/api/v1/sessions/{session_id}/messages"),
        )
        .json(&json!({
            "role": "user",
            "content_text": "benchmark approval content"
        }))
        .send()
        .await
        .context("create approval benchmark message request failed")?;
    assert_eq!(created_message.status(), StatusCode::CREATED);

    let created_run = gateway
        .request(Method::POST, format!("/api/v1/sessions/{session_id}/runs"))
        .json(&json!({}))
        .send()
        .await
        .context("create approval benchmark run request failed")?;
    assert_eq!(created_run.status(), StatusCode::CREATED);
    let run_json = json_body(created_run).await?;
    let run_id = run_json["run"]["run_id"]
        .as_str()
        .context("missing run_id in approval benchmark run response")?
        .to_string();

    let created_approval = gateway
        .request_with_operator(Method::POST, "/api/v1/approvals/request", "bench-op")
        .json(&json!({
            "run_id": run_id,
            "tool_name": "exec",
            "request_summary": "benchmark approval",
            "request_json": {"command":"echo benchmark"}
        }))
        .send()
        .await
        .context("create approval benchmark request failed")?;
    assert_eq!(created_approval.status(), StatusCode::CREATED);
    let approval_json = json_body(created_approval).await?;
    let approval_id = approval_json["approval"]["approval_id"]
        .as_str()
        .context("missing approval_id in approval benchmark response")?;

    let resolve_approval = gateway
        .request_with_operator(
            Method::POST,
            format!("/api/v1/approvals/{approval_id}/resolve"),
            "bench-op",
        )
        .json(&json!({"decision":"approve","decided_via":"benchmark"}))
        .send()
        .await
        .context("resolve approval benchmark request failed")?;
    assert_eq!(resolve_approval.status(), StatusCode::OK);
    Ok(())
}

async fn run_concurrent_health_burst(
    gateway: &GatewayProcess,
    total_requests: usize,
    concurrency: usize,
) -> Result<BurstStats> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .context("failed to create benchmark burst client")?;
    let base = gateway.http_base();
    let token = gateway.token().to_string();

    let started = Instant::now();
    let mut latencies = Vec::with_capacity(total_requests);
    let mut remaining = total_requests;

    while remaining > 0 {
        let batch_size = remaining.min(concurrency);
        remaining -= batch_size;

        let jobs = (0..batch_size)
            .map(|_| {
                let client = client.clone();
                let base = base.clone();
                let token = token.clone();
                tokio::spawn(async move {
                    let req_started = Instant::now();
                    let response = client
                        .get(format!("{base}/api/v1/health"))
                        .bearer_auth(&token)
                        .send()
                        .await
                        .context("concurrent health request failed")?;
                    if response.status() != StatusCode::OK {
                        anyhow::bail!("unexpected health status {}", response.status());
                    }
                    Ok::<f64, anyhow::Error>(req_started.elapsed().as_secs_f64() * 1000.0)
                })
            })
            .collect::<Vec<_>>();

        for result in join_all(jobs).await {
            latencies.push(result.context("health burst task panicked")??);
        }
    }

    let elapsed_secs = started.elapsed().as_secs_f64();
    let stats = LatencyStats::from_samples(&mut latencies)?;
    Ok(BurstStats {
        total_requests,
        concurrency,
        throughput_rps: total_requests as f64 / elapsed_secs.max(0.001),
        p95_ms: stats.p95_ms,
        p99_ms: stats.p99_ms,
        avg_ms: stats.avg_ms,
        max_ms: stats.max_ms,
    })
}

#[derive(Clone)]
struct NumquamBenchmarkState {
    resolve_calls: Arc<AtomicU64>,
}

struct NumquamBenchmarkStub {
    base_url: String,
    resolve_calls: Arc<AtomicU64>,
    task: tokio::task::JoinHandle<()>,
}

impl NumquamBenchmarkStub {
    async fn spawn() -> Result<Self> {
        let state = NumquamBenchmarkState {
            resolve_calls: Arc::new(AtomicU64::new(0)),
        };
        let app = Router::new()
            .route(
                "/api/integration/v1/context/build",
                post(numquam_benchmark_context_build),
            )
            .route(
                "/api/integration/v1/writeback/propose",
                post(numquam_benchmark_writeback_propose),
            )
            .route(
                "/api/integration/v1/writeback/resolve",
                post(numquam_benchmark_writeback_resolve),
            )
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .context("bind numquam benchmark stub failed")?;
        let addr = listener
            .local_addr()
            .context("read numquam benchmark stub local addr failed")?;
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

impl Drop for NumquamBenchmarkStub {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn numquam_benchmark_context_build(
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let request_id = payload
        .get("request_id")
        .and_then(|value| value.as_str())
        .unwrap_or("req_bench_context");
    Json(json!({
        "schema_version": "integration.v1",
        "request_id": request_id,
        "request_id_source": "client",
        "operation": "context.build",
        "ok": true,
        "degrade_mode": false,
        "warnings": [],
        "data": {
            "context_text": "benchmark memory context",
            "evidence": [{
                "evidence_id": "bench_ev_1",
                "section": "fact",
                "kind": "fact_card",
                "summary": "benchmark evidence",
                "citations": ["bench#1"],
                "confidence": 0.74
            }],
            "route": "ltm_light",
            "confidence": 0.74
        }
    }))
}

async fn numquam_benchmark_writeback_propose(
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let request_id = payload
        .get("request_id")
        .and_then(|value| value.as_str())
        .unwrap_or("req_bench_propose");
    let run_id = payload
        .get("run_id")
        .and_then(|value| value.as_str())
        .unwrap_or("run_bench");
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

async fn numquam_benchmark_writeback_resolve(
    AxumState(state): AxumState<NumquamBenchmarkState>,
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    state.resolve_calls.fetch_add(1, Ordering::Relaxed);
    let request_id = payload
        .get("request_id")
        .and_then(|value| value.as_str())
        .unwrap_or("req_bench_resolve");
    Json(json!({
        "schema_version": "integration.v1",
        "request_id": request_id,
        "request_id_source": "client",
        "operation": "writeback.resolve",
        "ok": true,
        "degrade_mode": false,
        "warnings": [],
        "data": {
            "proposal_id": "proposal_bench",
            "status": "approved",
            "already_resolved": false,
            "resolved_at_utc": "2026-02-19T00:00:00Z",
            "audit_ref": "audit_bench_resolve"
        }
    }))
}

#[derive(Debug, Clone, Copy)]
struct LatencyStats {
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    avg_ms: f64,
    max_ms: f64,
}

impl LatencyStats {
    fn from_samples(samples: &mut [f64]) -> Result<Self> {
        if samples.is_empty() {
            anyhow::bail!("latency sample set is empty");
        }
        samples.sort_by(|left, right| left.total_cmp(right));

        let p50_ms = percentile(samples, 0.50);
        let p95_ms = percentile(samples, 0.95);
        let p99_ms = percentile(samples, 0.99);
        let avg_ms = samples.iter().sum::<f64>() / samples.len() as f64;
        let max_ms = *samples.last().expect("samples checked non-empty");

        Ok(Self {
            p50_ms,
            p95_ms,
            p99_ms,
            avg_ms,
            max_ms,
        })
    }
}

fn percentile(sorted_samples: &[f64], quantile: f64) -> f64 {
    let max_index = sorted_samples.len() - 1;
    let index = (quantile * max_index as f64).round() as usize;
    sorted_samples[index.min(max_index)]
}

#[derive(Debug, Clone, Copy)]
struct BurstStats {
    total_requests: usize,
    concurrency: usize,
    throughput_rps: f64,
    p95_ms: f64,
    p99_ms: f64,
    avg_ms: f64,
    max_ms: f64,
}
