from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]


def test_run_longmemeval_eval_script_emits_predictions_and_traces(tmp_path: Path) -> None:
    dataset_path = tmp_path / "longmemeval_tiny.json"
    out_dir = tmp_path / "out"
    dataset = [
        {
            "question_id": "tiny_q1",
            "question_type": "single-session-user",
            "question": "What drink do I prefer during long debug sessions?",
            "answer": "tea",
            "question_date": "2026-03-22T12:00:00+00:00",
            "haystack_session_ids": ["sess_001"],
            "haystack_dates": ["2026-03-20T08:00:00+00:00"],
            "haystack_sessions": [
                [
                    {"role": "user", "content": "I prefer tea during long debug sessions."},
                    {"role": "assistant", "content": "I will remember that you prefer tea."},
                ]
            ],
            "answer_session_ids": ["sess_001"],
        }
    ]
    dataset_path.write_text(json.dumps(dataset), encoding="utf-8")

    result = subprocess.run(
        [
            sys.executable,
            "tools/run_longmemeval_eval.py",
            "--data-file",
            str(dataset_path),
            "--out-dir",
            str(out_dir),
            "--limit",
            "1",
            "--skip-official-eval",
        ],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
        timeout=300,
    )
    assert result.returncode == 0, result.stdout + "\n" + result.stderr

    predictions_path = out_dir / "predictions.jsonl"
    traces_path = out_dir / "traces.jsonl"
    summary_path = out_dir / "summary.json"
    assert predictions_path.exists()
    assert traces_path.exists()
    assert summary_path.exists()

    predictions = [json.loads(line) for line in predictions_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    traces = [json.loads(line) for line in traces_path.read_text(encoding="utf-8").splitlines() if line.strip()]
    summary = json.loads(summary_path.read_text(encoding="utf-8"))

    assert predictions[0]["question_id"] == "tiny_q1"
    assert isinstance(predictions[0]["hypothesis"], str)
    assert predictions[0]["hypothesis"].strip()
    assert "#" not in predictions[0]["hypothesis"]
    assert traces[0]["question_id"] == "tiny_q1"
    assert "package" in traces[0]
    assert "verified_reply" in traces[0]
    assert summary["dataset_cases"] == 1
    assert summary["provider"] == "mock"
    assert summary["official_eval"]["status"] == "skipped_flag"
