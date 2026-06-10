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

## 2026-06-10 — amend: purism surface-rule burndown measures (independent subagent)

New goal `compiler2-purism` added alongside `compiler2-selfhost`;
nothing in the existing selfhost measures moved (no targets loosened
or tightened). From the goal statement alone — compiler2/ must stop
violating docs/evident-purism.md's surface rules, survivors carrying
documented justification — added five measure scripts emitting 8
series (4 trends, 3 gates + 1 freshness gate), all live greps/counts
over `compiler2/*.ev` (comment- and string-stripped) except the
critic-report secondary signal:

| measure | kind | target | baseline 2026-06-10 |
|---|---|---|---|
| `v18_numbered_families` | trend | 0 unexempted | 49 |
| `v18_bind_peel_refs` | gate | 0 unexempted | 96 |
| `fsm_prefix_decls` | trend | 0 unexempted | 276 |
| `driver_lift_compositions` | gate | 0 unexempted | 24 |
| `v9_selection_chains` | trend | 0 unexempted | 62 |
| `cryptic_name_refs` | trend | 0 unexempted | 66 |
| `critic_v18_v9_findings` | trend (secondary) | 0 | 55 |
| `critic_report_age_days` | gate | ≤14 d | 0 |

"Documented justification" is mechanized as a ledger,
`docs/purism-exemptions.md` (`<CLASS> <file.ev|*> <token> — <reason>`
lines; absent today = zero exemptions); every measure subtracts
ledger-exempted instances and reports the exempted count in its label
so mass-exemption is visible. Carried-write hold chains (final
else-arm a `_carry`) are excluded from the V9 count by construction,
per the goal's own blessing. Targets set from the goal (all-zero /
fully-justified), not from current state. Scripts <50 ms, self-
time-boxed, read-only; exemption round-trip and determinism verified.

Observed while amending (not changed): `measures/code_quality.sh`
(goal `code-quality`) exists with no REVIEW.md or CHANGELOG entry —
it predates this amendment and needs a retroactive review row at
next human pass.

## 2026-06-08 — self-enforced timeouts (operator-directed)

Every measure script now hard-caps itself at 55 s via a
timeout-self-exec first line (GP_TIMEBOXED guard) — the budget holds
regardless of runner. Skill prompt.md records the pattern as a hard
rule. All five measures re-verified rc=0, <80 ms. No targets moved.
