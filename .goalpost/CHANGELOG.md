# Goalpost changelog — compiler2-selfhost

Auditable record of goalpost moves, per the goalpost skill's amend
rule.

## 2026-06-08 — authored (independent subagent)

Initial `.goalpost/` from the goal statement alone: 9 gates + 3
trends across the four rungs (conformance corpus, kernel-fixture
corpus, sample.ev equivalence, self-host stage2) plus the
legacy-import burndown. Artifact pattern for everything expensive;
freshness gates per artifact. See REVIEW.md.

## 2026-06-08 — operator-directed amendment (60s hard budget)

Operator rule: measure scripts must complete in <60s, always. The
measures already complied (<85 ms; they are artifact-parsers) —
amended the missing-artifact path from exit-1 (errored) to emitting
the honest zero state plus a maximally-stale freshness gate
(999999 h), so a fresh checkout shows red gates instead of a broken
ruler. The goalpost skill's prompt.md gained the hard 60 s ceiling
(no borderline tier) and the graceful missing-artifact rule.
Targets and harness semantics unchanged — no thresholds moved.
