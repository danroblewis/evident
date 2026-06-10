# Seam divergence ledger — frozen compiler.smt2 vs current semantics

**Status:** living ledger (opened 2026-06-10). Policy at the bottom;
the rows are the validation checklist for the wave-5 rebuild.

## What the seam is, and how old it is

The seam path is `kernel + compiler.smt2` (and the sister
`sample.smt2` for the sat-check verb). `compiler.smt2` was last
regenerated at **`a085f93` (2026-06-07)**, "via the bootstrap oracle
from current source" — three days and several semantics changes ago.
Since the bootstrap deletion there is no rebuild path: CLAUDE.md
declares it a checked-in binary artifact until the wave-5 loop closes
(`docs/plans/post-cutover-roadmap.md`). The bugfix-to-spec lineage
that DID continue lives on the `oracle` branch
(`scripts/build-oracle.sh` pin lineage: `c95710c` → `a1fd517` →
`292c7ef`), which builds the binary that compiles **compiler2** — it
never touches `compiler.smt2`. So every post-2026-06-07 semantics
change exists in zero, one, or two of {oracle, compiler2} but never
in the seam.

One structural fact keeps the ledger short: **pre-oracle text
transforms apply to seam inputs too.** The seam's flatten step
(`scripts/flatten-evident.sh:121`) pipes through
`scripts/passes/expand-fsm-autocarry.sh` and
`scripts/passes/lower-bounded-seq.sh`, so semantics implemented as
source transforms (the `fsm` keyword + autocarry, bounded-Seq
registries) reach the frozen artifact as plain claims and scalars —
handled upstream, no divergence. Only **artifact-internal** semantics
diverge.

## The ledger

Statuses: **DIVERGES** (seam disagrees with current pinned
semantics), **EXCUSED** (diverges, operationally allowlisted),
**PENDING** (semantics change in flight; seam will diverge on
landing), **SHARED GAP** (seam and current toolchain share the
documented gap), **UPSTREAM** (handled by pre-oracle transforms — no
divergence), **UNVERIFIED**.

| # | Semantics | Pinned by | Seam behavior | Status |
|---|-----------|-----------|---------------|--------|
| L1 | Bare claim mention HIDES unmapped internals; `..` lifts | `6af4042` (2026-06-10); conformance 139/140/141; oracle `a1fd517` | Seam still **lifts** on bare mention (the passthrough-synonym semantics the bootstrap lineage had drifted to) | DIVERGES |
| L2 | Claim headers as interface (header-join, explicit-only mapping, punning) | `docs/plans/claim-headers-interface.md` (approved 2026-06-10); oracle `292c7ef` (oracle branch; pin bump in a worktree, `77140fc`) | Seam keeps whole-body-implicit-interface. Backward compatible by design — diverges only for sources that **adopt** headers, which therefore must not be fed to the seam until the rebuild | PENDING |
| L3 | Record-typed carried fields (dotted field carry; pinned-constant manifest exclusion) | oracle `c95710c` (**2026-06-08** — one day AFTER the artifact's regeneration); CLAUDE.md state-carry section cites it as spec | Seam-compiled programs using a carried `r ∈ T` record dual lack the fix | DIVERGES |
| L4 | Arithmetic inside constructor args (`Exit(3 + 4)`) | CLAUDE.md enum/effect spec; compiler2 handles it by construction (STATE.md, 2026-06-08) | Seam emits `(Exit 3)` — the pre-cutover known issue (STATE.md "Open known issues"; real fix was `compiler/translate_ctor.ev`, never rebuilt) | EXCUSED — 16 feature dirs in the selfhost allowlist (`tests/conformance/features/runner.sh`, `DEFAULT_KNOWN_FAILS`) |
| L5 | Seq membership `x ∈ xs` | CLAUDE.md footgun box: silently dropped, vacuously SAT; `scripts/lint-seq-membership.sh` | Same lineage, same silent drop | SHARED GAP (documented + linted, not a divergence) |
| L6 | `fsm` keyword, autocarry, fsm composition carry threading | CLAUDE.md schema-keywords section; `scripts/passes/expand-fsm-autocarry.sh` | Transform rewrites `fsm` → `claim` + `_x` duals before the artifact sees the source | UPSTREAM |
| L7 | Bounded-Seq surfaces (`xs ∈ Seq(R)` + `#xs ≤ N`, keyed-projection PAIR, dynamic index) | `scripts/passes/lower-bounded-seq.sh` (production encoding since 2026-06-09, roadmap resolution) | Transform lowers to flat scalars before the artifact sees the source | UPSTREAM |
| L8 | Conformance 123 (quantifier in pin position) | the one known compiler2 failure (137/138 bar) | Seam behavior on 123 not established by any doc found in this audit | UNVERIFIED |

Nothing else surfaced from comparing CLAUDE.md's spec sections
against the artifact's vintage: the remaining spec sections (enums,
match/matches, generics, chained membership, record lifts,
single-writer rule, manifest header) predate 2026-06-07 or are
kernel-side, and the conformance corpus pins them. New rows belong
here the day any semantics commit lands without a seam rebuild —
adding the row is part of landing such a commit (the claim-headers
plan's migration section already does this for L2).

## Operational consequences

**Which test.sh phases run frozen artifacts.** Phase 3 (conformance,
`IMPL=selfhost` — flatten + `kernel compiler.smt2` per
`runner.sh compile_selfhost`), phase 4 (lang_tests via
`sample --all` = kernel + `sample.smt2`), phase 5 (kernel fixtures:
emit through the seam, then run), phase 6 (seam smoke), and phase 8
partially (the functionization gate asserts `compiler.smt2` itself
stays ~0 ms z3). There is no non-seam implementation left in-tree;
`runner.sh`'s `IMPL=bootstrap` default is a stale label — both
backends resolve to the same frozen artifact, differing only in
invocation protocol.

**L1's phase-3 effect.** Fixtures 139/140/141 landed 2026-06-10 and
are NOT in the selfhost allowlist. Analytically (not re-run in this
audit — a full phase-3 sweep through the seam is hours at seam emit
speeds): 139 and 140 should fail under lift semantics (two
`pick(out ↦ …)` sites share `picked` → UNSAT where independence is
expected), while 141 passes coincidentally (`..`-lift semantics are
unchanged). So phase 3 is expected red by two until either the rows
are allowlisted with a citation to this ledger, or the rebuild lands.

**The phase-5 pre-existing breakage.** The honest baseline
(STATE.md, 2026-06-08): 119 fixtures, **2 pass / 117 fail, 97 of
them emit TIMEOUTs** at the 120 s per-fixture cap
(`EVIDENT_KERNEL_FIXTURE_TIMEOUT`, `scripts/run-kernel-tests.sh:152`)
— the pre-existing `test_compiler_driver_*`-class grind,
fossil-identical (the POST-MERGE CORRECTION in
`docs/plans/kernel-tests-wedge-diagnosis.md` A/B'd it against the
pre-regeneration artifact). Operator-reported 2026-06-10: typical
seam emits run ~195 s against the 120 s cap. Probe re-run for this
ledger (2026-06-10, this machine): `test_ast_to_text.ev` emit killed
at a 300 s timeout with 0-byte output — matching the wedge
diagnosis's 2026-06-07 measurement of the same fixture (still
ticking at tick ~21,000 at 300 s; Class-A grinders ≈14 ms/tick to
the 100k-tick ceiling ≈ 25–31 min). Phase 5 is therefore a
**scoreboard for compiler2's eventual takeover of the corpus**
(STATE.md names it exactly that), not a regression gate on the seam:
its red is expected, bounded, and uninformative about new work.

## Policy

**Divergences are documented, not fixed.** The artifact cannot be
patched (no rebuild path; editing 2 MB of SMT-LIB by hand is not a
path), and the oracle-change rules (`scripts/build-oracle.sh`) keep
even the oracle to bugfix-to-spec — the seam stays where 2026-06-07
left it until the wave-5 rebuild loop closes (recognizer + codegen in
Evident → AOT to a fresh `compiler.smt2`, per CLAUDE.md "Editing the
self-hosted compiler").

Until then:

1. A semantics change is not done until its ledger row exists here
   (DIVERGES or PENDING) and any phase-3 allowlist change cites the
   row.
2. Sources that must compile through the seam (everything in
   `tests/kernel/`, `tests/seam/`, conformance under phase 3) may not
   rely on post-2026-06-07 artifact-internal semantics — today that
   means: no reliance on bare-mention hiding, no claim headers, no
   carried record duals.
3. **This ledger is the rebuild-validation checklist.** When a
   rebuilt `compiler.smt2`/`sample.smt2` candidate exists: every
   DIVERGES/PENDING row must flip green on its named fixtures
   (139/140/141 for L1; the claim-headers fixture set for L2; the
   record-carry conformance/unit fixtures for L3; the 16 allowlisted
   feature dirs for L4 — and the allowlist entries get deleted, which
   is the loud proof); every SHARED GAP row gets an explicit decision
   (fixed in the rebuild, or re-documented); every UPSTREAM row
   either keeps its transform in the pipeline or absorbs it into
   `compiler2/passes/` (the roadmap's passes-in-Evident deliverable);
   L8 gets measured instead of guessed. Then the phase-5 corpus
   stops being a scoreboard and becomes a gate again.
