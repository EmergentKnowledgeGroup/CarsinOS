#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from collections import Counter
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parents[1]
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

from engine.continuity import ContinuityBuilder, ContinuityStore
from engine.ingest import run_sqlite_import_job
from engine.ingest.parser import normalize_timestamp
from engine.memory import SqliteAtomStore
from engine.retrieval import ClaimVerifier, MemoryRetriever
from engine.responder import (
    ChatProviderConfig,
    build_provider,
    build_responder_messages,
    enforce_reply_contract,
    verify_reply_against_package,
)
from engine.runtime import RuntimeSession


def _utcnow() -> datetime:
    return datetime.now(timezone.utc)


def _stamp() -> str:
    return _utcnow().strftime("%Y%m%d_%H%M%S")


def _default_data_path() -> Path:
    return REPO_ROOT / "runtime" / "external" / "LongMemEval" / "data" / "longmemeval_oracle.json"


def _default_out_dir() -> Path:
    return REPO_ROOT / "runtime" / "evals" / f"longmemeval_{_stamp()}"


def _coerce_text(value: Any) -> str:
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, list):
        parts = [str(item).strip() for item in value if str(item).strip()]
        return "\n".join(parts).strip()
    return str(value or "").strip()


def _normalize_turn_role(value: Any) -> str:
    role = str(value or "").strip().lower()
    if role == "user":
        return "user"
    if role == "assistant":
        return "assistant"
    return ""


def _turn_timestamp(raw_date: Any, turn_index: int) -> str:
    parsed = normalize_timestamp(raw_date)
    if parsed is None:
        fallback = _utcnow() + timedelta(seconds=max(0, int(turn_index)))
        return fallback.isoformat()
    shifted = parsed + timedelta(seconds=max(0, int(turn_index)))
    return shifted.isoformat()


def _indexed(items: Any, index: int, default: Any = "") -> Any:
    if isinstance(items, list) and 0 <= index < len(items):
        return items[index]
    return default


def build_longmemeval_export(example: dict[str, Any]) -> dict[str, Any]:
    question_id = str(example.get("question_id") or "").strip() or "longmemeval_case"
    question_date = str(example.get("question_date") or "").strip()
    conversations: list[dict[str, Any]] = []

    sessions = example.get("haystack_sessions")
    if not isinstance(sessions, list):
        raise ValueError("LongMemEval example missing haystack_sessions[]")

    for session_index, session_turns in enumerate(sessions):
        if not isinstance(session_turns, list):
            continue
        raw_session_id = _indexed(example.get("haystack_session_ids"), session_index, f"session_{session_index + 1}")
        session_id = str(raw_session_id or f"session_{session_index + 1}").strip() or f"session_{session_index + 1}"
        session_date = _indexed(example.get("haystack_dates"), session_index, question_date)
        message_rows: list[dict[str, Any]] = []
        for turn_index, turn in enumerate(session_turns):
            if not isinstance(turn, dict):
                continue
            role = _normalize_turn_role(turn.get("role"))
            if not role:
                continue
            text = _coerce_text(turn.get("content"))
            if not text:
                continue
            message_rows.append(
                {
                    "id": f"{question_id}:{session_id}:m{turn_index:04d}",
                    "role": role,
                    "text": text,
                    "time_iso": _turn_timestamp(session_date, turn_index),
                }
            )
        if not message_rows:
            continue
        conversations.append(
            {
                "id": f"{question_id}::{session_id}",
                "conversation_id": f"{question_id}::{session_id}",
                "create_time_iso": _turn_timestamp(session_date, 0),
                "messages": message_rows,
            }
        )

    return {
        "generated_at": question_date or _utcnow().isoformat(),
        "source": "LongMemEval",
        "question_id": question_id,
        "question": str(example.get("question") or "").strip(),
        "conversations": conversations,
    }


def _load_dataset(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(payload, list):
        raise ValueError("LongMemEval dataset must be a JSON array")
    cases = [item for item in payload if isinstance(item, dict)]
    if not cases:
        raise ValueError("LongMemEval dataset did not contain any object rows")
    return cases


def _build_continuity(store: SqliteAtomStore) -> ContinuityStore:
    continuity = ContinuityStore()
    shared_language_keys = store.list_shared_language_keys() if hasattr(store, "list_shared_language_keys") else []
    continuity.set_snapshot(
        ContinuityBuilder().build(store.list_atoms(), shared_language_keys=shared_language_keys)
    )
    return continuity


def run_longmemeval_case(
    example: dict[str, Any],
    *,
    workspace_dir: Path,
    memory_preference: str,
    provider_name: str,
    provider_base_url: str,
    provider_chat_path: str,
    provider_model: str,
    openai_api_key: str,
) -> tuple[dict[str, Any], dict[str, Any]]:
    export_payload = build_longmemeval_export(example)
    export_path = workspace_dir / "longmemeval_export.json"
    store_path = workspace_dir / "atoms.sqlite3"
    export_path.write_text(json.dumps(export_payload, ensure_ascii=False), encoding="utf-8")

    import_report = run_sqlite_import_job(input_path=export_path, sqlite_path=store_path)
    if not import_report.ok:
        raise RuntimeError(str(import_report.error_message or "LongMemEval import failed"))

    store = SqliteAtomStore(store_path)
    runtime: RuntimeSession | None = None
    try:
        continuity = _build_continuity(store)
        runtime = RuntimeSession(
            retriever=MemoryRetriever(store),
            verifier=ClaimVerifier(),
            continuity_store=continuity,
            short_term_enabled=False,
            enable_writeback=False,
            prewarm_caches=False,
        )
        question_id = str(example.get("question_id") or "").strip() or "longmemeval_case"
        question = str(example.get("question") or "").strip()
        package = runtime.build_context_package(
            question,
            memory_preference=memory_preference,
            session_id=None,
            package_version="v2",
            render_citations=False,
        )
        provider = build_provider(
            ChatProviderConfig(
                provider=provider_name,
                base_url=provider_base_url,
                api_key=openai_api_key,
                chat_path=provider_chat_path,
            )
        )
        messages = build_responder_messages(package)
        provider_response = provider.chat(
            messages=messages,
            model=provider_model,
        )
        hypothesis = enforce_reply_contract(package, str(provider_response.text or "").strip())
        hypothesis = _strip_package_citations(hypothesis, package)
        verified = verify_reply_against_package(package, hypothesis)
        prediction = {
            "question_id": question_id,
            "hypothesis": hypothesis,
        }
        trace_payload = {
            "question_id": question_id,
            "question_type": str(example.get("question_type") or "").strip(),
            "question": question,
            "answer": str(example.get("answer") or "").strip(),
            "question_date": str(example.get("question_date") or "").strip(),
            "package": package,
            "reply_text": hypothesis,
            "reply_text_raw": str(provider_response.text or "").strip(),
            "verified_reply": {
                "ok": bool(verified.ok),
                "reasons": list(verified.reasons),
                "found_citations": list(verified.found_citations),
                "unknown_citations": list(verified.unknown_citations),
                "inferred_decision": str(verified.inferred_decision or ""),
            },
            "provider": {
                "provider": str(provider_response.provider),
                "model": str(provider_response.model),
                "latency_ms": float(provider_response.latency_ms),
                "usage": dict(provider_response.usage or {}),
            },
            "import_report": import_report.to_dict(),
        }
        return prediction, trace_payload
    finally:
        if runtime is not None:
            runtime.close()
        store.close()


def _write_jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    text = "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows)
    path.write_text(text, encoding="utf-8")


def _strip_package_citations(text: str, package: dict[str, Any]) -> str:
    cleaned = str(text or "")
    evidence = package.get("ltm_evidence") if isinstance(package.get("ltm_evidence"), list) else []
    for item in evidence:
        if not isinstance(item, dict):
            continue
        for raw in list(item.get("citations") or []):
            token = str(raw or "").strip()
            if token:
                cleaned = cleaned.replace(token, " ")
    return " ".join(cleaned.split()).strip()


def _run_official_eval(
    *,
    dataset_path: Path,
    predictions_path: Path,
    out_dir: Path,
    judge_model: str,
) -> dict[str, Any]:
    eval_dir = REPO_ROOT / "runtime" / "external" / "LongMemEval" / "src" / "evaluation"
    result: dict[str, Any] = {
        "status": "skipped",
        "judge_model": judge_model,
        "eval_dir": str(eval_dir),
    }
    if not eval_dir.exists():
        result["status"] = "missing_eval_dir"
        return result
    if not os.getenv("OPENAI_API_KEY"):
        result["status"] = "skipped_no_openai_api_key"
        return result

    evaluate_cmd = [
        sys.executable,
        "evaluate_qa.py",
        judge_model,
        str(predictions_path),
        str(dataset_path),
    ]
    eval_proc = subprocess.run(
        evaluate_cmd,
        cwd=eval_dir,
        capture_output=True,
        text=True,
        check=False,
        timeout=3600,
    )
    result["evaluate_returncode"] = eval_proc.returncode
    result["evaluate_stdout"] = eval_proc.stdout
    result["evaluate_stderr"] = eval_proc.stderr
    result_file = Path(str(predictions_path) + f".eval-results-{judge_model}")
    result["eval_results_path"] = str(result_file)
    if eval_proc.returncode != 0 or not result_file.exists():
        result["status"] = "evaluate_failed"
        return result

    metrics_cmd = [
        sys.executable,
        "print_qa_metrics.py",
        str(result_file),
        str(dataset_path),
    ]
    metrics_proc = subprocess.run(
        metrics_cmd,
        cwd=eval_dir,
        capture_output=True,
        text=True,
        check=False,
        timeout=600,
    )
    result["metrics_returncode"] = metrics_proc.returncode
    result["metrics_stdout"] = metrics_proc.stdout
    result["metrics_stderr"] = metrics_proc.stderr
    result["status"] = "completed" if metrics_proc.returncode == 0 else "metrics_failed"
    (out_dir / "official_eval.stdout.txt").write_text(eval_proc.stdout, encoding="utf-8")
    (out_dir / "official_eval.stderr.txt").write_text(eval_proc.stderr, encoding="utf-8")
    (out_dir / "official_metrics.stdout.txt").write_text(metrics_proc.stdout, encoding="utf-8")
    (out_dir / "official_metrics.stderr.txt").write_text(metrics_proc.stderr, encoding="utf-8")
    return result


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Run MNO on LongMemEval and emit official prediction JSONL.")
    parser.add_argument(
        "--data-file",
        default=str(_default_data_path()),
        help="Path to LongMemEval JSON dataset file.",
    )
    parser.add_argument(
        "--out-dir",
        default=str(_default_out_dir()),
        help="Output directory for predictions, traces, and summary.",
    )
    parser.add_argument("--limit", type=int, default=0, help="Optional case limit. 0 runs the full dataset.")
    parser.add_argument(
        "--memory-preference",
        choices=["auto", "chat_first", "memory_assist"],
        default="memory_assist",
        help="Runtime memory preference used for each benchmark question.",
    )
    parser.add_argument(
        "--provider",
        default="mock",
        choices=["mock", "lmstudio", "openai"],
        help="Responder provider used to turn the MNO context package into answer text.",
    )
    parser.add_argument(
        "--provider-base-url",
        default=os.getenv("LMSTUDIO_BASE_URL", "http://127.0.0.1:1234"),
        help="Base URL for lmstudio/openai-compatible providers when applicable.",
    )
    parser.add_argument(
        "--provider-chat-path",
        default=os.getenv("LMSTUDIO_CHAT_PATH", "/api/v1/chat"),
        help="Chat path for lmstudio/openai-compatible providers when applicable.",
    )
    parser.add_argument(
        "--provider-model",
        default=os.getenv("LMSTUDIO_MODEL", "mock-longmemeval-reader"),
        help="Responder model name passed to the selected provider.",
    )
    parser.add_argument(
        "--openai-api-key",
        default=os.getenv("OPENAI_API_KEY", ""),
        help="API key for the openai provider.",
    )
    parser.add_argument(
        "--judge-model",
        default="gpt-4o",
        help="Official LongMemEval evaluator model short name.",
    )
    parser.add_argument(
        "--skip-official-eval",
        action="store_true",
        help="Only generate predictions and MNO traces. Skip upstream evaluation.",
    )
    parser.add_argument(
        "--fail-fast",
        action="store_true",
        help="Stop on the first case error instead of writing an empty hypothesis and continuing.",
    )
    parser.add_argument(
        "--plan-only",
        action="store_true",
        help="Print resolved inputs and exit.",
    )
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    dataset_path = Path(args.data_file).expanduser().resolve()
    out_dir = Path(args.out_dir).expanduser().resolve()

    if args.plan_only:
        print(f"dataset_path={dataset_path}")
        print(f"out_dir={out_dir}")
        print(f"memory_preference={args.memory_preference}")
        print(f"provider={args.provider}")
        print(f"provider_model={args.provider_model}")
        print(f"official_eval={'off' if args.skip_official_eval else 'auto'}")
        return 0

    if not dataset_path.exists():
        print(f"error=dataset path not found: {dataset_path}")
        return 2

    dataset = _load_dataset(dataset_path)
    if args.limit > 0:
        dataset = dataset[: max(0, int(args.limit))]
    if not dataset:
        print("error=dataset is empty after applying limit")
        return 2

    out_dir.mkdir(parents=True, exist_ok=True)
    predictions: list[dict[str, Any]] = []
    traces: list[dict[str, Any]] = []
    decision_counts: Counter[str] = Counter()
    started = time.perf_counter()
    error_count = 0

    for index, example in enumerate(dataset, start=1):
        question_id = str(example.get("question_id") or "").strip() or f"case_{index:04d}"
        case_started = time.perf_counter()
        try:
            with tempfile.TemporaryDirectory(prefix=f"longmemeval_{question_id}_", dir=out_dir) as tmp_dir:
                prediction, trace_payload = run_longmemeval_case(
                    example,
                    workspace_dir=Path(tmp_dir),
                    memory_preference=str(args.memory_preference),
                    provider_name=str(args.provider),
                    provider_base_url=str(args.provider_base_url),
                    provider_chat_path=str(args.provider_chat_path),
                    provider_model=str(args.provider_model),
                    openai_api_key=str(args.openai_api_key),
                )
            predictions.append(prediction)
            traces.append(trace_payload)
            decision_counts.update(
                [
                    str(
                        (
                            (trace_payload.get("package") or {})
                            .get("service_verdict", {})
                            .get("decision")
                        )
                        or "UNKNOWN"
                    )
                ]
            )
        except Exception as exc:
            error_count += 1
            if args.fail_fast:
                raise
            predictions.append({"question_id": question_id, "hypothesis": ""})
            traces.append(
                {
                    "question_id": question_id,
                    "question_type": str(example.get("question_type") or "").strip(),
                    "question": str(example.get("question") or "").strip(),
                    "answer": str(example.get("answer") or "").strip(),
                    "question_date": str(example.get("question_date") or "").strip(),
                    "error": str(exc),
                }
            )
            decision_counts.update(["ERROR"])
        if index == 1 or index % 25 == 0 or index == len(dataset):
            elapsed = time.perf_counter() - case_started
            print(f"[{index}/{len(dataset)}] question_id={question_id} case_s={elapsed:.2f}")

    predictions_path = out_dir / "predictions.jsonl"
    traces_path = out_dir / "traces.jsonl"
    _write_jsonl(predictions_path, predictions)
    _write_jsonl(traces_path, traces)

    official_eval: dict[str, Any]
    if args.skip_official_eval:
        official_eval = {"status": "skipped_flag"}
    else:
        official_eval = _run_official_eval(
            dataset_path=dataset_path,
            predictions_path=predictions_path,
            out_dir=out_dir,
            judge_model=str(args.judge_model),
        )

    total_duration_s = time.perf_counter() - started
    summary = {
        "ok": True,
        "dataset_path": str(dataset_path),
        "dataset_cases": len(dataset),
        "memory_preference": str(args.memory_preference),
        "provider": str(args.provider),
        "provider_model": str(args.provider_model),
        "predictions_path": str(predictions_path),
        "traces_path": str(traces_path),
        "decision_counts": dict(sorted(decision_counts.items())),
        "error_count": error_count,
        "duration_s": round(total_duration_s, 3),
        "official_eval": official_eval,
        "generated_at": _utcnow().isoformat(),
    }
    summary_path = out_dir / "summary.json"
    summary_path.write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")

    print(f"predictions_path={predictions_path}")
    print(f"traces_path={traces_path}")
    print(f"summary_path={summary_path}")
    print(f"cases={len(dataset)} errors={error_count} duration_s={total_duration_s:.2f}")
    print(f"official_eval_status={official_eval.get('status')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
