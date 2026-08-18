from __future__ import annotations

from datetime import datetime, timezone

from engine.contracts import AtomType, CandidateAtom, SourceRef
from engine.memory import AtomStatus, AtomStore, EventType


def test_memory_store_end_to_end_flow() -> None:
    store = AtomStore(salience_half_life_days=180)

    first = CandidateAtom(
        candidate_id="cand_1",
        atom_type=AtomType.EPISODE,
        canonical_text="We discussed continuity plans.",
        source_refs=[SourceRef(source_id="conv_1", timestamp=datetime.now(timezone.utc))],
        entities=["user", "assistant"],
        topics=["continuity"],
        confidence=0.82,
        salience=0.71,
    )
    second = CandidateAtom(
        candidate_id="cand_2",
        atom_type=AtomType.EPISODE,
        canonical_text="We discussed continuity plans.",
        source_refs=[SourceRef(source_id="conv_2", timestamp=datetime.now(timezone.utc))],
        entities=["user", "assistant"],
        topics=["continuity"],
        confidence=0.91,
        salience=0.75,
    )

    atom = store.add_candidate(first)
    reinforced = store.add_candidate(second)
    assert atom.atom_id == reinforced.atom_id
    assert reinforced.support_count == 2
    assert reinforced.status == AtomStatus.ACTIVE

    alternate = store.add_candidate(
        CandidateAtom(
            candidate_id="cand_3",
            atom_type=AtomType.EPISODE,
            canonical_text="We canceled continuity plans.",
            source_refs=[SourceRef(source_id="conv_3", timestamp=datetime.now(timezone.utc))],
            entities=["user", "assistant"],
            topics=["continuity"],
            confidence=0.77,
            salience=0.68,
        )
    )

    store.mark_conflict(reinforced.atom_id, alternate.atom_id, reason="timeline divergence")
    events = store.ledger.all_events()
    assert any(event.event_type == EventType.ADD for event in events)
    assert any(event.event_type == EventType.REINFORCE for event in events)
    assert any(event.event_type == EventType.CONFLICT for event in events)


def test_conflict_marking_is_idempotent() -> None:
    store = AtomStore()
    first = store.add_candidate(
        CandidateAtom(
            candidate_id="cand_a",
            atom_type=AtomType.EPISODE,
            canonical_text="Dark mode is banned from this test.",
            source_refs=[SourceRef(source_id="conv_a", timestamp=datetime.now(timezone.utc))],
            entities=["user"],
            topics=["policy"],
            confidence=0.8,
            salience=0.6,
        )
    )
    second = store.add_candidate(
        CandidateAtom(
            candidate_id="cand_b",
            atom_type=AtomType.EPISODE,
            canonical_text="Dark mode is required for this test.",
            source_refs=[SourceRef(source_id="conv_b", timestamp=datetime.now(timezone.utc))],
            entities=["user"],
            topics=["policy"],
            confidence=0.8,
            salience=0.6,
        )
    )

    store.mark_conflict(first.atom_id, second.atom_id, reason="first")
    store.mark_conflict(first.atom_id, second.atom_id, reason="retry")

    assert store.get_atom(first.atom_id).contradiction_count == 1
    assert store.get_atom(second.atom_id).contradiction_count == 1
    conflict_events = [event for event in store.ledger.all_events() if event.event_type == EventType.CONFLICT]
    assert len(conflict_events) == 1


def test_supersede_atom_creates_successor_for_same_dedupe_key() -> None:
    store = AtomStore()
    original_candidate = CandidateAtom(
        candidate_id="cand_same_old",
        atom_type=AtomType.EPISODE,
        canonical_text="Keep this exact text.",
        source_refs=[SourceRef(source_id="conv_old", timestamp=datetime.now(timezone.utc))],
        entities=["user"],
        topics=["continuity"],
        confidence=0.8,
        salience=0.6,
    )
    original = store.add_candidate(original_candidate)

    successor = store.supersede_atom(original.atom_id, original_candidate, reason="same_text_update")

    assert successor.atom_id != original.atom_id
    assert successor.version_of == original.atom_id
    assert store.get_atom(original.atom_id).status == AtomStatus.SUPERSEDED
