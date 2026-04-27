"""
Phase 12: Session state — the evidence base lifecycle.

Facts are asserted monotonically — once established, never retracted.
The Session object is the single source of truth for everything that
has been derived so far.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from .evidence import Evidence


class Session:
    """
    Manages the growing evidence base across multiple queries.

    Facts are asserted monotonically — once established, never retracted.
    This mirrors Evident's semantics: the evidence base only grows.
    """

    def __init__(self):
        self.established: list[Evidence] = []
        self.facts: dict[str, Any] = {}  # ground facts (name → Python value)

    # ------------------------------------------------------------------
    # Evidence management
    # ------------------------------------------------------------------

    def add_evidence(self, ev: Evidence) -> None:
        """Add a derived evidence node to the base."""
        self.established.append(ev)

    def is_established(self, claim: str, bindings: dict | None = None) -> bool:
        """
        Check if a claim is already in the evidence base.

        Parameters
        ----------
        claim:
            The name of the claim/schema that was established.
        bindings:
            If provided, all entries must match the evidence's bindings.
            If None, any evidence for this claim is accepted.
        """
        for ev in self.established:
            if ev.claim != claim:
                continue
            if bindings is None:
                return True
            if all(ev.bindings.get(k) == v for k, v in bindings.items()):
                return True
        return False

    # ------------------------------------------------------------------
    # Ground fact management
    # ------------------------------------------------------------------

    def assert_fact(self, name: str, value: Any) -> None:
        """
        Assert a ground fact into the session.

        Monotonic: if the name is already bound to the same value, this is a
        no-op.  Asserting a different value raises ValueError (no retraction).
        """
        if name in self.facts:
            existing = self.facts[name]
            if existing != value:
                raise ValueError(
                    f"Cannot retract or change fact {name!r}: "
                    f"already asserted as {existing!r}, tried {value!r}."
                )
            return
        self.facts[name] = value

    def get_fact(self, name: str) -> Any | None:
        """Return the asserted value for a ground fact, or None."""
        return self.facts.get(name)

    # ------------------------------------------------------------------
    # Snapshot / inspection
    # ------------------------------------------------------------------

    def all_claims(self) -> list[str]:
        """Return the list of claim names in the evidence base (may repeat)."""
        return [ev.claim for ev in self.established]

    def evidence_for(self, claim: str) -> list[Evidence]:
        """Return all evidence nodes for the given claim name."""
        return [ev for ev in self.established if ev.claim == claim]

    def __len__(self) -> int:
        return len(self.established)

    def __repr__(self) -> str:
        return (
            f"Session(facts={list(self.facts.keys())}, "
            f"established={len(self.established)})"
        )
