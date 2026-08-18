#!/usr/bin/env python3
"""Run Telegram/Discord channel soak probes and emit a resilience report."""

from __future__ import annotations

import argparse
import json
import math
import os
import sys
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple
from urllib import error as url_error
from urllib import parse as url_parse
from urllib import request as url_request


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def utc_compact_ts() -> str:
    return datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")


def now_ms() -> int:
    return int(time.time() * 1000)


def percentile(values: List[float], pct: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = (pct / 100.0) * (len(ordered) - 1)
    low = int(math.floor(rank))
    high = int(math.ceil(rank))
    if low == high:
        return ordered[low]
    fraction = rank - low
    return ordered[low] * (1.0 - fraction) + ordered[high] * fraction


def bump(counter: Dict[str, int], key: str) -> None:
    counter[key] = counter.get(key, 0) + 1


def parse_json_maybe(raw: str) -> Optional[Dict[str, Any]]:
    try:
        parsed = json.loads(raw) if raw else {}
    except json.JSONDecodeError:
        return None
    return parsed if isinstance(parsed, dict) else None


@dataclass
class HttpResult:
    status: int
    body: Optional[Dict[str, Any]]
    raw_body: str
    latency_ms: float
    error_text: Optional[str] = None


class GatewayClient:
    def __init__(self, base_url: str, token: str, timeout_seconds: float) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.timeout_seconds = timeout_seconds

    def request_json(
        self,
        method: str,
        path: str,
        payload: Optional[Dict[str, Any]] = None,
        query: Optional[Dict[str, Any]] = None,
    ) -> HttpResult:
        query_text = ""
        if query:
            encoded = url_parse.urlencode(
                {key: value for key, value in query.items() if value is not None}
            )
            if encoded:
                query_text = f"?{encoded}"
        url = f"{self.base_url}{path}{query_text}"
        body_data = None
        headers = {
            "accept": "application/json",
            "authorization": f"Bearer {self.token}",
        }
        if payload is not None:
            body_data = json.dumps(payload).encode("utf-8")
            headers["content-type"] = "application/json"

        req = url_request.Request(url, data=body_data, method=method.upper(), headers=headers)
        started = time.perf_counter()
        try:
            with url_request.urlopen(req, timeout=self.timeout_seconds) as response:
                raw_body = response.read().decode("utf-8", errors="replace")
                status = int(response.getcode())
                latency_ms = (time.perf_counter() - started) * 1000.0
                return HttpResult(
                    status=status,
                    body=parse_json_maybe(raw_body),
                    raw_body=raw_body,
                    latency_ms=latency_ms,
                )
        except url_error.HTTPError as exc:
            raw_body = exc.read().decode("utf-8", errors="replace")
            latency_ms = (time.perf_counter() - started) * 1000.0
            return HttpResult(
                status=int(exc.code),
                body=parse_json_maybe(raw_body),
                raw_body=raw_body,
                latency_ms=latency_ms,
                error_text=f"http_error:{exc.code}",
            )
        except url_error.URLError as exc:
            latency_ms = (time.perf_counter() - started) * 1000.0
            return HttpResult(
                status=0,
                body=None,
                raw_body="",
                latency_ms=latency_ms,
                error_text=f"transport_error:{exc.reason}",
            )


@dataclass
class ProviderStats:
    attempted: int = 0
    succeeded: int = 0
    failed: int = 0
    status_counts: Dict[str, int] = field(default_factory=dict)
    decision_counts: Dict[str, int] = field(default_factory=dict)
    latencies_ms: List[float] = field(default_factory=list)
    outbound_reply_status_counts: Dict[str, int] = field(default_factory=dict)
    outbound_delivery_mode_counts: Dict[str, int] = field(default_factory=dict)
    run_ids: List[str] = field(default_factory=list)
    errors: List[str] = field(default_factory=list)
    audit_cursor_ms: int = 0
    seen_audit_event_ids: set = field(default_factory=set)

    def latency_summary(self) -> Dict[str, float]:
        return {
            "p50": round(percentile(self.latencies_ms, 50), 3),
            "p95": round(percentile(self.latencies_ms, 95), 3),
            "p99": round(percentile(self.latencies_ms, 99), 3),
            "max": round(max(self.latencies_ms) if self.latencies_ms else 0.0, 3),
        }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Run channel soak probes and emit a resilience report."
    )
    parser.add_argument(
        "--base-url",
        default=os.getenv("CARSINOS_BASE_URL", "http://127.0.0.1:7341"),
        help="Gateway base URL.",
    )
    parser.add_argument(
        "--token",
        default=os.getenv("CARSINOS_AUTH_TOKEN", ""),
        help="Gateway bearer token. Required unless --dry-run.",
    )
    parser.add_argument(
        "--output-dir",
        default="runtime/channels/reports",
        help="Directory where logs/reports are written.",
    )
    parser.add_argument(
        "--label",
        default="channel-soak",
        help="Label included in generated events and output artifacts.",
    )
    parser.add_argument(
        "--iterations",
        type=int,
        default=None,
        help="Iteration count. If omitted, computed from --duration-hours/--interval-seconds.",
    )
    parser.add_argument(
        "--duration-hours",
        type=float,
        default=168.0,
        help="Target soak duration in hours when --iterations is omitted.",
    )
    parser.add_argument(
        "--interval-seconds",
        type=int,
        default=300,
        help="Delay between iterations.",
    )
    parser.add_argument(
        "--request-timeout-seconds",
        type=float,
        default=15.0,
        help="Per-request timeout.",
    )
    parser.add_argument(
        "--model-provider",
        default="mock",
        help="Model provider for inbound run_immediately payloads.",
    )
    parser.add_argument(
        "--model-id",
        default="mock-echo-v1",
        help="Model ID for inbound run_immediately payloads.",
    )
    parser.add_argument(
        "--telegram-chat-id",
        type=int,
        default=int(os.getenv("CARSINOS_TELEGRAM_CHAT_ID", "0") or 0),
        help="Telegram chat ID for inbound probes.",
    )
    parser.add_argument(
        "--telegram-user-id",
        type=int,
        default=int(os.getenv("CARSINOS_TELEGRAM_USER_ID", "0") or 0),
        help="Telegram user ID for inbound probes.",
    )
    parser.add_argument(
        "--discord-channel-id",
        default=os.getenv("CARSINOS_DISCORD_CHANNEL_ID", ""),
        help="Discord channel ID for inbound probes.",
    )
    parser.add_argument(
        "--discord-author-id",
        default=os.getenv("CARSINOS_DISCORD_AUTHOR_ID", ""),
        help="Discord author ID for inbound probes.",
    )
    parser.add_argument(
        "--operator-peer-id",
        default=os.getenv("CARSINOS_OPERATOR_PEER_ID", "soak-runner"),
        help="Peer ID used for channel approval resolution probes.",
    )
    parser.add_argument(
        "--min-success-rate",
        type=float,
        default=0.99,
        help="Minimum success rate per provider (0-1).",
    )
    parser.add_argument(
        "--max-failure-rate",
        type=float,
        default=0.01,
        help="Maximum failure rate per provider (0-1).",
    )
    parser.add_argument(
        "--skip-telegram",
        action="store_true",
        help="Skip Telegram probes.",
    )
    parser.add_argument(
        "--skip-discord",
        action="store_true",
        help="Skip Discord probes.",
    )
    parser.add_argument(
        "--skip-approval-roundtrip",
        action="store_true",
        help="Skip channel approval callback roundtrip probes.",
    )
    parser.add_argument(
        "--no-sleep",
        action="store_true",
        help="Disable per-iteration sleep (useful for tests/smoke).",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Do not call gateway endpoints; emit planned configuration/report only.",
    )
    return parser


def ensure_iterations(args: argparse.Namespace, parser: argparse.ArgumentParser) -> int:
    if args.interval_seconds <= 0:
        parser.error("--interval-seconds must be > 0")
    if args.iterations is not None:
        if args.iterations <= 0:
            parser.error("--iterations must be > 0")
        return args.iterations
    if args.duration_hours <= 0:
        parser.error("--duration-hours must be > 0 when --iterations is omitted")
    return max(1, int(math.ceil((args.duration_hours * 3600.0) / args.interval_seconds)))


def configured_providers(args: argparse.Namespace, parser: argparse.ArgumentParser) -> List[str]:
    providers: List[str] = []
    if not args.skip_telegram:
        if args.telegram_chat_id and args.telegram_user_id:
            providers.append("telegram")
        elif not args.dry_run:
            parser.error(
                "telegram requires --telegram-chat-id and --telegram-user-id (or use --skip-telegram)"
            )
    if not args.skip_discord:
        if args.discord_channel_id.strip() and args.discord_author_id.strip():
            providers.append("discord")
        elif not args.dry_run:
            parser.error(
                "discord requires --discord-channel-id and --discord-author-id (or use --skip-discord)"
            )
    if not providers:
        parser.error("no providers configured for soak run")
    return providers


def read_runtime_status(client: GatewayClient, log) -> Dict[str, Dict[str, Any]]:
    response = client.request_json("GET", "/api/v1/channels/runtime/status")
    if response.status != 200 or not response.body:
        log(
            f"runtime status fetch failed status={response.status} error={response.error_text or ''}"
        )
        return {}
    items = response.body.get("items")
    if not isinstance(items, list):
        return {}
    out: Dict[str, Dict[str, Any]] = {}
    for item in items:
        if not isinstance(item, dict):
            continue
        provider = str(item.get("provider", "")).strip().lower()
        if not provider:
            continue
        out[provider] = {
            "healthy": bool(item.get("healthy", False)),
            "lifecycle_state": item.get("lifecycle_state"),
            "reconnect_attempts": int(item.get("reconnect_attempts", 0) or 0),
            "updated_at": int(item.get("updated_at", 0) or 0),
            "detail": item.get("detail"),
            "last_error": item.get("last_error"),
        }
    return out


def probe_inbound(
    client: GatewayClient,
    provider: str,
    args: argparse.Namespace,
    index: int,
    label: str,
) -> HttpResult:
    text = f"[{label}] iteration={index} provider={provider} ts={utc_now_iso()}"
    if provider == "telegram":
        payload = {
            "chat_id": args.telegram_chat_id,
            "user_id": args.telegram_user_id,
            "text": text,
            "is_group_chat": False,
            "mentions_bot": False,
            "reply_to_bot": False,
            "run_immediately": True,
            "model_provider": args.model_provider,
            "model_id": args.model_id,
        }
        return client.request_json("POST", "/api/v1/channels/telegram/inbound", payload)
    payload = {
        "channel_id": args.discord_channel_id.strip(),
        "author_id": args.discord_author_id.strip(),
        "text": text,
        "mentions_bot": False,
        "is_dm": True,
        "run_immediately": True,
        "model_provider": args.model_provider,
        "model_id": args.model_id,
    }
    return client.request_json("POST", "/api/v1/channels/discord/inbound", payload)


def poll_ingest_audit(
    client: GatewayClient,
    provider: str,
    cursor_ms: int,
    stats: ProviderStats,
    log,
) -> int:
    action = f"channel.{provider}.ingest"
    response = client.request_json(
        "GET",
        "/api/v1/security/audit",
        query={"action": action, "created_after": cursor_ms, "limit": 200},
    )
    if response.status != 200 or not response.body:
        if response.status != 0:
            log(
                f"audit fetch failed provider={provider} status={response.status} error={response.error_text or ''}"
            )
        return cursor_ms
    items = response.body.get("items")
    if not isinstance(items, list):
        return cursor_ms

    max_seen = cursor_ms
    for item in items:
        if not isinstance(item, dict):
            continue
        event_id = str(item.get("event_id", "")).strip()
        if not event_id or event_id in stats.seen_audit_event_ids:
            continue
        stats.seen_audit_event_ids.add(event_id)
        created_at = int(item.get("created_at", 0) or 0)
        if created_at >= max_seen:
            max_seen = created_at + 1
        metadata_raw = item.get("metadata_json")
        metadata: Dict[str, Any] = {}
        if isinstance(metadata_raw, str) and metadata_raw.strip():
            try:
                parsed = json.loads(metadata_raw)
                if isinstance(parsed, dict):
                    metadata = parsed
            except json.JSONDecodeError:
                pass
        outbound_status = str(metadata.get("outbound_reply_status", "")).strip()
        if outbound_status:
            bump(stats.outbound_reply_status_counts, outbound_status)
        delivery_mode = str(metadata.get("outbound_delivery_mode", "")).strip()
        if delivery_mode:
            bump(stats.outbound_delivery_mode_counts, delivery_mode)
    return max_seen


def post_json_expect(
    client: GatewayClient,
    path: str,
    payload: Dict[str, Any],
    expected_status: int,
) -> Tuple[bool, str, Optional[Dict[str, Any]]]:
    response = client.request_json("POST", path, payload=payload)
    if response.status != expected_status or not response.body:
        return (
            False,
            f"{path} status={response.status} expected={expected_status} error={response.error_text or response.raw_body}",
            response.body,
        )
    return True, "", response.body


def run_approval_roundtrip(
    client: GatewayClient,
    provider: str,
    args: argparse.Namespace,
    label: str,
) -> Dict[str, Any]:
    target = (
        f"telegram:{args.telegram_chat_id}"
        if provider == "telegram"
        else f"discord:{args.discord_channel_id.strip()}"
    )
    command = f"tool.channel_send {target}|[{label}] approval probe"

    ok, error_text, create_session = post_json_expect(
        client,
        "/api/v1/sessions",
        {"title": f"{label}-approval-{provider}"},
        201,
    )
    if not ok:
        return {"status": "failed", "error": error_text}
    session = create_session.get("session", {}) if create_session else {}
    session_id = str(session.get("session_id", "")).strip()
    if not session_id:
        return {"status": "failed", "error": "session_id missing from create session"}

    ok, error_text, _ = post_json_expect(
        client,
        f"/api/v1/sessions/{session_id}/messages",
        {"role": "user", "content_text": command},
        201,
    )
    if not ok:
        return {"status": "failed", "error": error_text, "session_id": session_id}

    ok, error_text, run_response = post_json_expect(
        client,
        f"/api/v1/sessions/{session_id}/runs",
        {},
        201,
    )
    if not ok:
        return {"status": "failed", "error": error_text, "session_id": session_id}

    run_id = str((run_response or {}).get("run", {}).get("run_id", "")).strip()
    if not run_id:
        return {"status": "failed", "error": "run_id missing from create run"}

    approvals_response = client.request_json(
        "GET",
        "/api/v1/approvals",
        query={"status": "requested", "limit": 200},
    )
    if approvals_response.status != 200 or not approvals_response.body:
        return {
            "status": "failed",
            "error": f"approvals list status={approvals_response.status}",
            "run_id": run_id,
        }
    approvals = approvals_response.body.get("items")
    if not isinstance(approvals, list):
        return {"status": "failed", "error": "approvals list malformed", "run_id": run_id}
    approval_id = ""
    for item in approvals:
        if not isinstance(item, dict):
            continue
        if str(item.get("run_id", "")).strip() == run_id:
            approval_id = str(item.get("approval_id", "")).strip()
            if approval_id:
                break
    if not approval_id:
        return {"status": "failed", "error": "approval not found for run", "run_id": run_id}

    action_payload = (
        f"approval:approve:{approval_id}"
        if provider == "telegram"
        else f"approval|approve|{approval_id}"
    )
    resolve_response = client.request_json(
        "POST",
        "/api/v1/channels/approvals/resolve",
        payload={
            "provider": provider,
            "action_payload": action_payload,
            "actor_peer_id": args.operator_peer_id,
        },
    )
    if resolve_response.status != 200 or not resolve_response.body:
        return {
            "status": "failed",
            "error": f"resolve status={resolve_response.status}",
            "run_id": run_id,
            "approval_id": approval_id,
        }
    approval_status = str(
        (resolve_response.body.get("approval", {}) or {}).get("status", "")
    ).strip()
    if approval_status != "approved":
        return {
            "status": "failed",
            "error": f"unexpected approval status={approval_status}",
            "run_id": run_id,
            "approval_id": approval_id,
        }
    return {
        "status": "passed",
        "run_id": run_id,
        "approval_id": approval_id,
        "provider": provider,
    }


def serialize_provider_stats(
    stats: ProviderStats,
    initial_runtime: Dict[str, Any],
    final_runtime: Dict[str, Any],
) -> Dict[str, Any]:
    attempted = stats.attempted
    succeeded = stats.succeeded
    failed = stats.failed
    success_rate = (succeeded / attempted) if attempted else 0.0
    failure_rate = (failed / attempted) if attempted else 1.0
    reconnect_delta = int(final_runtime.get("reconnect_attempts", 0) or 0) - int(
        initial_runtime.get("reconnect_attempts", 0) or 0
    )
    return {
        "attempted": attempted,
        "succeeded": succeeded,
        "failed": failed,
        "success_rate": round(success_rate, 6),
        "failure_rate": round(failure_rate, 6),
        "latency_ms": stats.latency_summary(),
        "status_counts": stats.status_counts,
        "decision_counts": stats.decision_counts,
        "outbound_reply_status_counts": stats.outbound_reply_status_counts,
        "outbound_delivery_mode_counts": stats.outbound_delivery_mode_counts,
        "run_ids_sample": stats.run_ids[:20],
        "error_sample": stats.errors[:20],
        "runtime_health_initial": initial_runtime.get("healthy"),
        "runtime_health_final": final_runtime.get("healthy"),
        "runtime_reconnect_attempts_initial": int(
            initial_runtime.get("reconnect_attempts", 0) or 0
        ),
        "runtime_reconnect_attempts_final": int(
            final_runtime.get("reconnect_attempts", 0) or 0
        ),
        "runtime_reconnect_delta": reconnect_delta,
    }


def run(argv: Optional[Sequence[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    args.label = args.label.strip() or "channel-soak"
    args.base_url = args.base_url.strip()
    if not (args.base_url.startswith("http://") or args.base_url.startswith("https://")):
        parser.error("--base-url must start with http:// or https://")
    if not 0.0 <= args.min_success_rate <= 1.0:
        parser.error("--min-success-rate must be between 0 and 1")
    if not 0.0 <= args.max_failure_rate <= 1.0:
        parser.error("--max-failure-rate must be between 0 and 1")

    iterations = ensure_iterations(args, parser)
    providers = configured_providers(args, parser)
    if not args.dry_run and not args.token.strip():
        parser.error("--token (or CARSINOS_AUTH_TOKEN) is required unless --dry-run")

    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)
    timestamp = utc_compact_ts()
    log_path = output_dir / f"channel-soak-{timestamp}.log"
    summary_path = output_dir / f"channel-soak-{timestamp}.json"
    latest_path = output_dir / "channel-soak-latest.json"

    def log(message: str) -> None:
        line = f"[{utc_now_iso()}] {message}"
        print(line)
        with log_path.open("a", encoding="utf-8") as fh:
            fh.write(line + "\n")

    started_at_iso = utc_now_iso()
    started_at_ms = now_ms()
    log(f"channel soak start label={args.label} providers={providers} iterations={iterations}")

    if args.dry_run:
        dry_summary = {
            "timestamp_utc": utc_now_iso(),
            "status": "dry_run",
            "label": args.label,
            "base_url": args.base_url,
            "providers": providers,
            "iterations": iterations,
            "interval_seconds": args.interval_seconds,
            "paths": {
                "log_file": str(log_path),
                "report_file": str(summary_path),
                "latest_file": str(latest_path),
            },
        }
        summary_path.write_text(json.dumps(dry_summary, indent=2) + "\n", encoding="utf-8")
        latest_path.write_text(json.dumps(dry_summary, indent=2) + "\n", encoding="utf-8")
        log("dry-run completed")
        return 0

    client = GatewayClient(args.base_url, args.token.strip(), args.request_timeout_seconds)
    initial_runtime = read_runtime_status(client, log)
    provider_stats: Dict[str, ProviderStats] = {name: ProviderStats() for name in providers}
    for name in providers:
        provider_stats[name].audit_cursor_ms = started_at_ms

    for index in range(1, iterations + 1):
        log(f"iteration {index}/{iterations} begin")
        for provider in providers:
            stats = provider_stats[provider]
            response = probe_inbound(client, provider, args, index, args.label)
            stats.attempted += 1
            stats.latencies_ms.append(response.latency_ms)
            bump(stats.status_counts, str(response.status))

            decision = ""
            run_id = ""
            if response.body:
                decision = str(response.body.get("decision", "")).strip().lower()
                run_id = str(response.body.get("run_id", "")).strip()
            bump(stats.decision_counts, decision or "unknown")
            if run_id:
                stats.run_ids.append(run_id)

            success = (
                response.status == 200
                and decision == "accepted"
                and bool(run_id)
                and not response.error_text
            )
            if success:
                stats.succeeded += 1
            else:
                stats.failed += 1
                error_detail = (
                    response.error_text
                    or f"status={response.status} decision={decision} body={response.raw_body[:200]}"
                )
                stats.errors.append(f"iteration={index} provider={provider} {error_detail}")
                log(f"probe failure iteration={index} provider={provider} {error_detail}")

            stats.audit_cursor_ms = poll_ingest_audit(
                client, provider, stats.audit_cursor_ms, stats, log
            )

        if index < iterations and not args.no_sleep:
            time.sleep(args.interval_seconds)

    final_runtime = read_runtime_status(client, log)

    approval_roundtrip: Dict[str, Any] = {"enabled": not args.skip_approval_roundtrip}
    if args.skip_approval_roundtrip:
        approval_roundtrip["status"] = "skipped"
    else:
        all_passed = True
        for provider in providers:
            result = run_approval_roundtrip(client, provider, args, args.label)
            approval_roundtrip[provider] = result
            if result.get("status") != "passed":
                all_passed = False
                log(
                    f"approval roundtrip failed provider={provider} error={result.get('error', 'unknown')}"
                )
        approval_roundtrip["status"] = "passed" if all_passed else "failed"

    ended_at_iso = utc_now_iso()
    ended_at_ms = now_ms()

    serialized_providers: Dict[str, Any] = {}
    reasons: List[str] = []
    for provider in providers:
        stats = provider_stats[provider]
        initial = initial_runtime.get(provider, {})
        final = final_runtime.get(provider, {})
        serialized = serialize_provider_stats(stats, initial, final)
        serialized_providers[provider] = serialized

        if serialized["attempted"] <= 0:
            reasons.append(f"{provider}:no_attempts")
            continue
        if serialized["success_rate"] < args.min_success_rate:
            reasons.append(
                f"{provider}:success_rate_below_min({serialized['success_rate']}<{args.min_success_rate})"
            )
        if serialized["failure_rate"] > args.max_failure_rate:
            reasons.append(
                f"{provider}:failure_rate_above_max({serialized['failure_rate']}>{args.max_failure_rate})"
            )
        if serialized["runtime_health_final"] is False:
            reasons.append(f"{provider}:runtime_unhealthy_final_state")

    if approval_roundtrip.get("enabled") and approval_roundtrip.get("status") != "passed":
        reasons.append("approval_roundtrip_failed")

    status = "green" if not reasons else "red"

    summary = {
        "timestamp_utc": utc_now_iso(),
        "status": status,
        "label": args.label,
        "base_url": args.base_url,
        "window": {
            "started_at_utc": started_at_iso,
            "ended_at_utc": ended_at_iso,
            "started_at_ms": started_at_ms,
            "ended_at_ms": ended_at_ms,
            "duration_seconds": max(0, (ended_at_ms - started_at_ms) // 1000),
        },
        "thresholds": {
            "min_success_rate": args.min_success_rate,
            "max_failure_rate": args.max_failure_rate,
        },
        "config": {
            "iterations": iterations,
            "interval_seconds": args.interval_seconds,
            "request_timeout_seconds": args.request_timeout_seconds,
            "model_provider": args.model_provider,
            "model_id": args.model_id,
            "providers": providers,
            "skip_approval_roundtrip": args.skip_approval_roundtrip,
        },
        "providers": serialized_providers,
        "approval_roundtrip": approval_roundtrip,
        "runtime_status_initial": initial_runtime,
        "runtime_status_final": final_runtime,
        "reasons": reasons,
        "paths": {
            "log_file": str(log_path),
            "report_file": str(summary_path),
            "latest_file": str(latest_path),
        },
    }

    summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    latest_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    log(f"summary report written: {summary_path}")
    log(f"latest report written: {latest_path}")

    if status != "green":
        log("channel soak completed with red status")
        return 1
    log("channel soak completed with green status")
    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    try:
        return run(argv)
    except KeyboardInterrupt:
        print("interrupted", file=sys.stderr)
        return 130


if __name__ == "__main__":
    raise SystemExit(main())
