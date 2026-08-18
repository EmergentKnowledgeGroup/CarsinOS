from __future__ import annotations

from datetime import datetime, timezone
from pathlib import Path

from engine.contracts import SourceRef
from engine.memory.proposal_store import (
    InMemoryProposalStore,
    ProposalCandidate,
    ProposalKind,
    ProposalStatus,
    SqliteProposalStore,
)


def _candidate(
    *,
    kind: ProposalKind,
    text: str,
    reason_code: str,
    source_id: str,
    session_id: str = "sess_proposal",
    role: str = "assistant",
    confidence: float = 0.66,
) -> ProposalCandidate:
    return ProposalCandidate(
        kind=kind,
        canonical_text=text,
        source_refs=[
            SourceRef(
                source_id=source_id,
                message_id=f"{source_id}:m1",
                timestamp=datetime(2026, 3, 24, 0, 0, tzinfo=timezone.utc),
                span_start=0,
                span_end=max(1, len(text)),
            )
        ],
        source_role=role,
        session_id=session_id,
        reason_code=reason_code,
        confidence=confidence,
        metadata={"source_role": role},
    )


def _exercise_store(store: InMemoryProposalStore | SqliteProposalStore) -> dict[str, object]:
    first = store.upsert_candidate(
        _candidate(
            kind=ProposalKind.OTHER_PERSON_INTERNAL_STATE,
            text="I think Thao feels defeated about MonkeyBars.",
            reason_code="other_person_internal_state",
            source_id="turn_1:assistant",
        ),
        reason="turn_proposal_only",
    )
    reinforced = store.upsert_candidate(
        _candidate(
            kind=ProposalKind.OTHER_PERSON_INTERNAL_STATE,
            text="I think Thao feels defeated about MonkeyBars.",
            reason_code="other_person_internal_state",
            source_id="turn_2:assistant",
        ),
        reason="turn_proposal_only",
    )
    records = store.list_records()
    diagnostics = store.diagnostics_snapshot()
    events = [event.event_type.value for event in store.list_events()]
    return {
        "record_id": first.record_id,
        "status": first.status.value,
        "reinforcement_count": reinforced.reinforcement_count,
        "record_count": len(records),
        "pending_count": diagnostics["pending_count"],
        "event_types": events,
    }


def _close_if_supported(store: InMemoryProposalStore | SqliteProposalStore) -> None:
    closer = getattr(store, "close", None)
    if callable(closer):
        closer()


def test_inmemory_and_sqlite_proposal_stores_have_parity(tmp_path: Path) -> None:
    mem_store = InMemoryProposalStore()
    sqlite_store = SqliteProposalStore(tmp_path / "atoms.proposals.sqlite3")
    try:
        mem_summary = _exercise_store(mem_store)
        sqlite_summary = _exercise_store(sqlite_store)
    finally:
        _close_if_supported(mem_store)
        _close_if_supported(sqlite_store)

    assert mem_summary == sqlite_summary


def test_proposal_store_reinforces_exact_match_and_stays_pending() -> None:
    store = InMemoryProposalStore()
    first = store.upsert_candidate(
        _candidate(
            kind=ProposalKind.IDENTITY_SUMMARY,
            text="Xander is the kind of person who builds at 5am because he cares deeply.",
            reason_code="identity_summary",
            source_id="turn_a",
        ),
        reason="turn_proposal_only",
    )
    second = store.upsert_candidate(
        _candidate(
            kind=ProposalKind.IDENTITY_SUMMARY,
            text="Xander is the kind of person who builds at 5am because he cares deeply.",
            reason_code="identity_summary",
            source_id="turn_b",
        ),
        reason="turn_proposal_only",
    )

    assert first.record_id == second.record_id
    assert second.reinforcement_count == 2
    assert second.status is ProposalStatus.PENDING
