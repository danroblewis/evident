# BLOCKED: `recs` as a claim-header slot (ambient-recs retirement)

Date: 2026-06-10. Context: the claim-headers pilot
(`docs/plans/claim-headers-interface.md`; DriverBroadcast landed as
pilot A). Pilot B was to add `recs` to the headers of
`RtIdxOf`/`RtSortOf`/`RtFieldAcc` in `compiler2/driver_record.ev`,
retiring the ambient-recs debt pinned in
`docs/critic-reports/compiler2-baseline.md`.

## The wall

`recs ∈ Seq(RecTypeEntry)` is a bounded Seq-of-records, and
`scripts/passes/lower-bounded-seq.sh` scalarizes it PRE-ORACLE: the
flattened source the oracle (and compiler2-stage1) sees contains only
`recs_0_name`, `recs_0_sort`, … — the name `recs` does not survive the
lowering (verified on the flattened driver, 2026-06-10: `RtIdxOf`'s
body reads `recs_0_name`/`recs_0_sort`; no `recs ∈` membership
remains).

Consequences for a `recs` header slot:

- A pun `RtIdxOf(nm ↦ x, idx ↦ y, recs)` cannot bind: post-lowering
  there is no `recs` const for the mapping value to resolve to, and
  the claim body never mentions `recs` — only its lowered scalars,
  which resolve as FREE NAMES in the caller (names-match), exactly as
  before.
- The header slot would therefore be silently-vacuous surface (a V2
  smell): wiring that reads as binding the registry but binds nothing.

This is precisely the plan's open question 1 (context bundles —
"depends on record-slot mapping ergonomics; design later, do not block
headers on it"). The registry IS a context bundle.

## Today's state (debt restated, not retired)

The Rt* lookups keep reading `recs` ambiently via free-name
resolution. Headers do not regress this — free names a claim never
declares still resolve in the caller — but they cannot yet make the
dependency explicit.

## Retirement paths (pick when designing context bundles)

1. **Record-slot mapping in the lowering**: teach
   `lower-bounded-seq.sh` (or its compiler2-native successor) to
   expand a Seq-of-records mapping `recs ↦ recs` into the per-scalar
   bindings (`recs_0_name ↦ recs_0_name`, …) at the call seam — the
   pun then means what it says.
2. **Context-bundle record**: one record instance passed as a single
   slot once record-slot mapping ergonomics exist (the plan's stated
   direction for the driver's wide-context components).

Until one lands, ambient `recs` stays an allowed exception, documented
at the consuming module headers (it already is — driver_record.ev's
PRODUCES note names the ambient read).
