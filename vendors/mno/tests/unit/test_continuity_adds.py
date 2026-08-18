from __future__ import annotations

from pathlib import Path

from engine.runtime.continuity_adds import (
    append_action_log,
    default_continuity_adds_state,
    load_continuity_adds_state,
    persist_continuity_adds_state,
    record_retrieval_feedback,
)


def test_continuity_adds_state_persists_feedback_and_action_log(tmp_path: Path) -> None:
    path = tmp_path / "continuity_adds_state.json"
    state = default_continuity_adds_state()

    record_retrieval_feedback(
        state,
        item_id="card_mem_1",
        item_kind="episode",
        feedback="useful",
        session_id="alpha",
        query_text="What happened to Lyra during the build night?",
        metadata={"memory_layer": "published"},
        max_entries=4,
        max_query_chars=24,
    )
    append_action_log(
        state,
        action_type="retrieval_feedback_recorded",
        summary="Recorded useful feedback for an episode card.",
        session_id="alpha",
        metadata={"item_id": "card_mem_1"},
        max_entries=4,
    )
    persist_continuity_adds_state(path, state)

    loaded = load_continuity_adds_state(path, max_feedback_entries=4, max_action_entries=4)
    feedback_rows = list(loaded.get("retrieval_feedback") or [])
    action_rows = list(loaded.get("action_log") or [])
    assert len(feedback_rows) == 1
    assert feedback_rows[0]["item_id"] == "card_mem_1"
    assert feedback_rows[0]["feedback"] == "useful"
    assert feedback_rows[0]["query_text"] == "What happened to Lyra d…"
    assert feedback_rows[0]["metadata"]["memory_layer"] == "published"
    assert len(action_rows) == 1
    assert action_rows[0]["action_type"] == "retrieval_feedback_recorded"
    assert action_rows[0]["metadata"]["item_id"] == "card_mem_1"


def test_continuity_adds_state_trims_feedback_and_actions_to_bounds() -> None:
    state = default_continuity_adds_state()
    for index in range(6):
        record_retrieval_feedback(
            state,
            item_id=f"atom_{index}",
            item_kind="atom",
            feedback="wrong",
            session_id="alpha",
            query_text=f"query {index}",
            metadata={},
            max_entries=3,
            max_query_chars=64,
        )
        append_action_log(
            state,
            action_type="feedback",
            summary=f"feedback {index}",
            session_id="alpha",
            metadata={},
            max_entries=2,
        )
    feedback_rows = list(state.get("retrieval_feedback") or [])
    action_rows = list(state.get("action_log") or [])
    assert [row["item_id"] for row in feedback_rows] == ["atom_3", "atom_4", "atom_5"]
    assert [row["summary"] for row in action_rows] == ["feedback 4", "feedback 5"]
