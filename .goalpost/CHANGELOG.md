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
2026-06-10 — operator approved compiler2-purism measures; hashes locked in REVIEW.md

## 2026-06-10 — amend: combined prefix denylist + retire obsolete lift measure (operator design decision)

Two moves in `compiler2-purism`, from an operator design decision
(2026-06-10). Only `.goalpost/` touched (measure + CHANGELOG +
REVIEW). Source untouched. Hashes left UNLOCKED for re-approval (the
`purism_namespacing.sh` hash in REVIEW.md's lock block is now stale and
must be re-approved before md runs it).

### 1. ENHANCED `fsm_prefix_decls` (trend) — now a COMBINED count

`measures/purism_namespacing.sh`. The measure previously counted only
decl-position names prefixed by the *auto-derived component word(s)* of
their own `fsm DriverXyz` (DriverGroup→`group_*`, etc.). It now ALSO
counts decl-position names beginning with a fixed ABBREVIATION denylist
that does not match any component word but is known namespacing debt:

    mp_ sv_ il_ rv_ rd_ rc_ ww_ pg_ d_pe d_m_ d_lk
    vf_ vfc_ ed_ stl_ stv_ qset_ bcast_

These abbreviations are disjoint from every auto-derived component
prefix (e.g. `bcast_` vs the full-word `broadcast_` that the component
half already catches), so the two halves never double-count — verified
(351 combined = 276 component, unchanged + 75 denylist). Denylist names
are matched decl-aware across multi-name decls (`a, b ∈ T` counts both),
with comments + string literals stripped first (matching the
cryptic-names measure). Still a trend → 0 unexempted; still subtracts
the `docs/purism-exemptions.md` V5 ledger (denylist survivors use the
same `V5 <file.ev> <prefix_*> — <reason>` line). The measure-id is
UNCHANGED (`fsm_prefix_decls`), so its history series is preserved.

- BEFORE: `fsm_prefix_decls` value 276 (component prefixes only).
- AFTER:  `fsm_prefix_decls` value **351** (combined; +75 denylist).
- LOOSENED? No — strictly TIGHTENED. The same target (0) now has to
  drive 75 more decl-position abbreviations to zero, not just the
  component-word ones. The component half is byte-for-byte unchanged.

Re-baseline against current main: **351** (deterministic, triple-run;
33 ms; valid single-line JSON).

### 2. RETIRED `driver_lift_compositions` (gate) — `..`-lift is now BLESSED

OPERATOR DECISION REVERSAL. This gate counted `..Driver*` lift lines in
`compiler2/driver.ev` with target 0, on the premise that `..`-lifts
should become header-based bare mentions. That premise is REVERSED:
de-prefixing is a pure rename that KEEPS the `..` lifts, and the
claim-headers conversion approach was abandoned as worse for
wide-interface components. So a target of 0 lifts is WRONG — `..`-lift
count is no longer a quality signal and there is no surviving criterion
that makes a particular lift "problematic".

Action: the measure is RETIRED (preferred over repurpose — there is no
problematic-lift criterion left to flag). The script
`measures/purism_namespacing.sh` no longer emits the
`driver_lift_compositions` line at all; md will drop that series. The
V11 ledger class it consumed (`V11 driver.ev <Schema>`) is now dead and
no measure reads it.

- BEFORE: `driver_lift_compositions` gate, value 24, target 0.
- AFTER:  series removed — no measure drives `..` lifts toward 0.
- This is a LOOSENING (a gate disappears), and it is the explicitly
  operator-directed point of the amendment: the old gate encoded a
  decision that has been reversed.

Verification: re-ran all compiler2-purism measures
(`purism_namespacing.sh`, `purism_cryptic_names.sh`, `purism_v18_*`,
`purism_v9_*`, `purism_critic_signal.sh`) — each emits valid JSON, is
deterministic (double/triple-run identical), <60 ms, self-time-boxed,
read-only. No other purism measure moved.
