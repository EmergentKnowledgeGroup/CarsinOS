from __future__ import annotations

import importlib.util
from pathlib import Path
import sys


def _load_module():
    root = Path(__file__).resolve().parents[2]
    path = root / "tools" / "run_longmemeval_eval.py"
    spec = importlib.util.spec_from_file_location("run_longmemeval_eval", path)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules["run_longmemeval_eval"] = module
    spec.loader.exec_module(module)
    return module


def test_build_longmemeval_export_creates_wrapper_conversations() -> None:
    module = _load_module()
    try:
        example = {
            "question_id": "q_demo",
            "question": "What drink do I prefer?",
            "question_date": "2026-03-22T12:00:00+00:00",
            "haystack_session_ids": ["s1", "s2"],
            "haystack_dates": [
                "2026-03-20T10:00:00+00:00",
                "2026-03-21T11:00:00+00:00",
            ],
            "haystack_sessions": [
                [
                    {"role": "user", "content": "Remember that I prefer tea at night."},
                    {"role": "assistant", "content": "I will remember the tea preference."},
                ],
                [
                    {"role": "user", "content": "Also note I like calm debug sessions."},
                    {"role": "assistant", "content": "Calm debug sessions noted."},
                ],
            ],
        }

        payload = module.build_longmemeval_export(example)

        assert payload["question_id"] == "q_demo"
        assert len(payload["conversations"]) == 2
        assert payload["conversations"][0]["id"] == "q_demo::s1"
        first_messages = payload["conversations"][0]["messages"]
        assert first_messages[0]["role"] == "user"
        assert "prefer tea" in first_messages[0]["text"].lower()
        assert first_messages[0]["time_iso"].startswith("2026-03-20T10:00:00")
        assert payload["conversations"][1]["messages"][1]["role"] == "assistant"
    finally:
        sys.modules.pop("run_longmemeval_eval", None)
