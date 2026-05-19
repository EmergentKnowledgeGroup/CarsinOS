from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from engine.contracts import SourceRef
from engine.memory.provisional_store import (
    InMemoryProvisionalMemoryStore,
    ProvisionalMemoryCandidate,
    ProvisionalMemoryKind,
    ProvisionalMemoryStatus,
    SqliteProvisionalMemoryStore,
)


def _candidate(
    *,
    kind: ProvisionalMemoryKind,
    text: str,
    source_id: str,
    session_id: str = "sess_demo",
    role: str = "user",
    confidence: float = 0.84,
    salience: float = 0.72,
    stability: float = 0.18,
) -> ProvisionalMemoryCandidate:
    return ProvisionalMemoryCandidate(
        kind=kind,
        canonical_text=text,
        source_refs=[
            SourceRef(
                source_id=source_id,
                message_id=f"{source_id}:m1",
                timestamp=datetime(2026, 3, 23, 23, 0, tzinfo=timezone.utc),
                span_start=0,
                span_end=max(1, len(text)),
            )
        ],
        source_role=role,
        session_id=session_id,
        confidence=confidence,
        salience=salience,
        stability=stability,
        metadata={"source_role": role},
    )


def _exercise_store(store: InMemoryProvisionalMemoryStore | SqliteProvisionalMemoryStore) -> dict[str, object]:
    first = store.upsert_candidate(
        _candidate(
            kind=ProvisionalMemoryKind.FACT,
            text="Thao finally came around on MonkeyBars and is in.",
            source_id="turn_1:user",
        ),
        reason="turn_auto_write",
    )
    reinforced = store.upsert_candidate(
        _candidate(
            kind=ProvisionalMemoryKind.FACT,
            text="Thao finally came around on MonkeyBars and is in.",
            source_id="turn_2:user",
        ),
        reason="turn_auto_write",
    )
    self_claim = store.upsert_candidate(
        _candidate(
            kind=ProvisionalMemoryKind.SELF_CLAIM,
            text="I trust Z deeply.",
            source_id="turn_2:assistant",
            role="assistant",
            stability=0.12,
        ),
        reason="turn_auto_write",
    )
    superseded = store.supersede_record(
        self_claim.record_id,
        _candidate(
            kind=ProvisionalMemoryKind.SELF_CLAIM,
            text="I trust Z deeply and consistently.",
            source_id="turn_3:assistant",
            role="assistant",
            stability=0.20,
        ),
        reason="explicit_correction",
    )
    hits = store.search("What changed with MonkeyBars?", limit=4)
    events = [event.event_type.value for event in store.list_events()]
    counts = store.diagnostics_snapshot()
    return {
        "fact_record_id": first.record_id,
        "fact_reinforcement_count": reinforced.reinforcement_count,
        "fact_status": reinforced.status.value,
        "self_claim_status": store.get_record(self_claim.record_id).status.value,
        "replacement_supersedes": superseded.supersedes_record_id == self_claim.record_id,
        "search_top_id": hits[0].record.record_id if hits else "",
        "search_top_kind": hits[0].record.kind.value if hits else "",
        "event_types": events,
        "active_count": counts["active_count"],
        "superseded_count": counts["superseded_count"],
        "total_count": counts["total_count"],
    }


def _close_if_supported(store: InMemoryProvisionalMemoryStore | SqliteProvisionalMemoryStore) -> None:
    closer = getattr(store, "close", None)
    if callable(closer):
        closer()


def test_inmemory_and_sqlite_provisional_memory_stores_have_parity(tmp_path: Path) -> None:
    mem_store = InMemoryProvisionalMemoryStore()
    sqlite_store = SqliteProvisionalMemoryStore(tmp_path / "atoms.provisional.sqlite3")
    try:
        mem_summary = _exercise_store(mem_store)
        sqlite_summary = _exercise_store(sqlite_store)
    finally:
        _close_if_supported(mem_store)
        _close_if_supported(sqlite_store)

    assert mem_summary == sqlite_summary


def test_provisional_store_reinforces_exact_match_instead_of_creating_duplicate() -> None:
    store = InMemoryProvisionalMemoryStore()
    first = store.upsert_candidate(
        _candidate(kind=ProvisionalMemoryKind.PREFERENCE, text="Xander likes perfect DOS fonts.", source_id="turn_a"),
        reason="turn_auto_write",
    )
    second = store.upsert_candidate(
        _candidate(kind=ProvisionalMemoryKind.PREFERENCE, text="Xander likes perfect DOS fonts.", source_id="turn_b"),
        reason="turn_auto_write",
    )

    assert first.record_id == second.record_id
    assert second.reinforcement_count == 2
    assert second.last_reinforced_at is not None


def test_provisional_store_preserves_utc_timestamps_and_audit_history(tmp_path: Path) -> None:
    store = SqliteProvisionalMemoryStore(tmp_path / "timestamps.provisional.sqlite3")
    try:
        record = store.upsert_candidate(
            _candidate(kind=ProvisionalMemoryKind.PLAN, text="We will revisit this tomorrow morning.", source_id="turn_plan"),
            reason="turn_auto_write",
        )
        fetched = store.get_record(record.record_id)
        events = store.list_events(record_id=record.record_id)
    finally:
        store.close()

    assert fetched.created_at.tzinfo is not None
    assert fetched.updated_at.tzinfo is not None
    assert fetched.created_at.utcoffset() == timezone.utc.utcoffset(fetched.created_at)
    assert events
    assert events[0].timestamp.tzinfo is not None


def test_provisional_store_search_filters_out_superseded_records() -> None:
    store = InMemoryProvisionalMemoryStore()
    original = store.upsert_candidate(
        _candidate(kind=ProvisionalMemoryKind.SELF_CLAIM, text="I love Y.", source_id="turn_1:assistant", role="assistant"),
        reason="turn_auto_write",
    )
    replacement = store.supersede_record(
        original.record_id,
        _candidate(
            kind=ProvisionalMemoryKind.SELF_CLAIM,
            text="I love Y and keep choosing it.",
            source_id="turn_2:assistant",
            role="assistant",
        ),
        reason="explicit_correction",
    )

    hits = store.search("love Y", limit=4)

    assert hits
    assert hits[0].record.record_id == replacement.record_id
    assert all(hit.record.record_id != original.record_id for hit in hits)
    assert store.get_record(original.record_id).status is ProvisionalMemoryStatus.SUPERSEDED


def test_provisional_store_search_scores_stay_bounded() -> None:
    store = InMemoryProvisionalMemoryStore()
    store.upsert_candidate(
        _candidate(
            kind=ProvisionalMemoryKind.FACT,
            text="MonkeyBars mattered because Thao finally came around on MonkeyBars.",
            source_id="turn_bound",
        ),
        reason="turn_auto_write",
    )

    hits = store.search("MonkeyBars", limit=2)

    assert hits
    assert 0.0 <= hits[0].score <= 1.0


def test_provisional_store_multi_hop_supersede_chain_keeps_only_latest_active() -> None:
    store = InMemoryProvisionalMemoryStore()
    first = store.upsert_candidate(
        _candidate(
            kind=ProvisionalMemoryKind.PLAN,
            text="I'm taking the job.",
            source_id="turn_1:user",
        ),
        reason="turn_auto_write",
    )
    second = store.supersede_record(
        first.record_id,
        _candidate(
            kind=ProvisionalMemoryKind.CORRECTION,
            text="Actually, I'm not taking the job.",
            source_id="turn_2:user",
        ),
        reason="explicit_correction",
    )
    third = store.supersede_record(
        second.record_id,
        _candidate(
            kind=ProvisionalMemoryKind.CORRECTION,
            text="Okay fine, I'm taking the job.",
            source_id="turn_3:user",
        ),
        reason="explicit_correction",
    )

    hits = store.search("taking the job", limit=6)

    assert hits
    assert hits[0].record.record_id == third.record_id
    assert all(hit.record.record_id not in {first.record_id, second.record_id} for hit in hits)
    assert store.get_record(first.record_id).status is ProvisionalMemoryStatus.SUPERSEDED
    assert store.get_record(second.record_id).status is ProvisionalMemoryStatus.SUPERSEDED
    assert third.supersedes_record_id == second.record_id


def test_provisional_store_marks_live_conflicts_and_keeps_them_searchable(tmp_path: Path) -> None:
    for store in (
        InMemoryProvisionalMemoryStore(),
        SqliteProvisionalMemoryStore(tmp_path / "conflicts.provisional.sqlite3"),
    ):
        try:
            first = store.upsert_candidate(
                _candidate(
                    kind=ProvisionalMemoryKind.FACT,
                    text="Thao is in on MonkeyBars now.",
                    source_id="turn_conflict_1:user",
                ),
                reason="turn_auto_write",
            )
            second = store.upsert_candidate(
                _candidate(
                    kind=ProvisionalMemoryKind.FACT,
                    text="Thao is out on MonkeyBars now.",
                    source_id="turn_conflict_2:user",
                ),
                reason="turn_auto_write",
            )

            left, right = store.mark_conflict(first.record_id, second.record_id, reason="manual_conflict")
            hits = store.search("MonkeyBars", limit=6)
            diagnostics = store.diagnostics_snapshot()

            assert left.status is ProvisionalMemoryStatus.CONFLICTED
            assert right.status is ProvisionalMemoryStatus.CONFLICTED
            assert right.record_id in left.conflict_with_record_ids
            assert left.record_id in right.conflict_with_record_ids
            assert {hit.record.record_id for hit in hits} == {left.record_id, right.record_id}
            assert diagnostics["conflicted_count"] == 2
            assert diagnostics["active_count"] == 0
        finally:
            _close_if_supported(store)
