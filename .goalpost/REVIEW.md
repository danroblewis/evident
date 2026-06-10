# Goalpost review — `compiler2-selfhost`

**Goal (restated):** compiler2 — the Evident program in `compiler2/`
that the Rust kernel runs, building output models through libz3
LibCalls and serializing with Z3 — becomes the project's real
compiler: it correctly compiles (units running correctly under the
kernel) the conformance corpus, the kernel fixture corpus, and
`compiler/sample.ev`; and ultimately compiles its own source into a
working compiler artifact, at which point the bootstrap-oracle
scaffolding and the legacy `compiler/` tree are deletable.

## Architecture: artifact pattern for everything expensive

A single fixture compiled through compiler2 takes **>10 minutes**
today (measured: `001-int-arithmetic-add` was still lexing at tick
6225 after 540 s — the per-tick Z3 path, functionizer reports
`functionized: false`). The corpora are 138 + 119 fixtures, and the
self-compile input is ~5 kLOC of Evident. None of that fits a measure
budget, so every corpus/self-host measure parses a JSON artifact
dropped by a harness in `.goalpost/bin/` (run from CI / cron / by
hand), and **freshness is its own gate** per artifact. Artifacts live
in `.goalpost/artifacts/` (gitignored). Measure scripts exit non-zero
(ruler broken) only when their artifact is missing entirely.

Harnesses record the per-fixture timeout cap and the stage1 builder
inside the artifact, so a reduced-budget or oracle-less run is
visible, not silent. Timeouts count as **not passed**.

## Measures

| measure | kind | rung | target | cadence | inspects | why a skeptic should believe it |
|---|---|---|---|---|---|---|
| `conformance_pass` | gate | det | live count of `tests/conformance/features/[0-9]*/` (138 today) | artifact ≤72 h, parse 300 s | `compiler2-conformance.json` from `bin/run-conformance.sh`: every fixture flattened → compiled by kernel+compiler2 → `expected/smt2-contains` substrings checked → emitted unit run under the kernel vs `expected/stdout`/`exit` | Pass requires real compiled output with the corpus's own expected shapes and runtime behaviour; the 12-line no-such-claim stub fails the contains/run checks. Target is counted from the live tree, so new fixtures raise the bar and a stale artifact can't cover them. |
| `conformance_failing` | trend | det | 0 | 300 s | same | Burndown = live total − artifact passed; unmeasured new fixtures count as failing. |
| `conformance_fresh` | gate | det | ≤72 h | 300 s | artifact `ts` | A stopped harness goes red on its own. |
| `kernel_fixtures_pass` | gate | det | live count of `tests/kernel/test_*.ev` (119 today) | artifact ≤72 h, parse 300 s | `compiler2-kernel.json` from `bin/run-kernel-corpus.sh`: per-fixture `-- expect:` stdout/exit headers parsed with `run-kernel-tests.sh` semantics; compile via compiler2; run unit; compare | Same construction as conformance; expectations come from the fixtures themselves, not from any agent's report. |
| `kernel_fixtures_failing` | trend | det | 0 | 300 s | same | Burndown. |
| `kernel_fixtures_fresh` | gate | det | ≤72 h | 300 s | artifact `ts` | — |
| `sample_ev_equiv` | gate (bool) | det | met when 1 | artifact ≤168 h, parse 300 s | `compiler2-sample.json` from `bin/run-sample.sh`: compiler2 compiles `compiler/sample.ev` (claim `main`); candidate AND the committed known-good `sample.smt2` are each run as sat-check drivers on reference inputs (`tests/lang_tests/test_enums_basic.ev`, `test_matches.ev`); their `(claim, sat/unsat)` sequences via `z3 -in` must match exactly | "Correctly compiles" is defined behaviourally against the committed artifact built from the same source — not "emitted something". Stub/empty/divergent candidates all fail. |
| `sample_ev_fresh` | gate | det | ≤168 h | 300 s | artifact `ts` | — |
| `selfhost_stage2_built` | gate (bool) | det | met when 1 | artifact ≤168 h, parse 300 s | `compiler2-selfhost.json` from `bin/run-selfhost.sh`: kernel+stage1 compiles flattened `compiler2/driver.ev` (claim `driver_main`); built requires manifest header AND >100 lines (the documented stub trap is ~12 lines) | Cannot be satisfied without the self-compile actually completing. |
| `selfhost_stage2_works` | gate (bool) | det | met when 1 | same artifact | stage2 itself compiles two smoke fixtures (`test_hello.ev` → "hello world"/exit 0; conformance 001 → smt2-contains + exit 7) and the units run correctly | This is the goal's "working compiler artifact from itself": stage2 must *function as a compiler*, not merely exist. |
| `selfhost_fresh` | gate | det | ≤168 h | 300 s | artifact `ts` | — |
| `legacy_compiler_imports` | trend | det | 0 | live, 300 s | transitive `import "…"` closure of `compiler2/*.ev`, counting distinct `compiler/*.ev` files reached (3 today: lexer, parser, translate_arith) | Direct check of the deletability clause: while compiler2's import closure reaches `compiler/`, deleting the legacy tree breaks compiler2. Runs in ms against the live tree. |

**Definition of done** = all gates green simultaneously: both corpora
fully passing under compiler2, sample.ev behaviourally equivalent,
stage2 built **and working**, all artifacts fresh, and the legacy
import closure at 0 (with `selfhost_stage2_works`, this is what makes
`scripts/build-oracle.sh`, `/usr/local/bin/evident-oracle`, and
`compiler/` deletable without loss of capability).

## Freshness targets

72 h for the two corpora (they parallelize; a full run is hours, not
days) and 168 h for the sample/self-host runs (single multi-hour
compiles that cannot parallelize internally). These are cadence
expectations for an actively-worked goal, not statements about current
pass rates.

## Harness cost & knobs

- `bin/run-conformance.sh` / `bin/run-kernel-corpus.sh`:
  `EVIDENT_C2_TIMEOUT` (default 1800 s/fixture), `EVIDENT_C2_JOBS`
  (default 8). Worst case ≈ total·cap/jobs.
- `bin/run-sample.sh`: `EVIDENT_C2_SAMPLE_TIMEOUT` (default 14400 s).
- `bin/run-selfhost.sh`: `EVIDENT_C2_SELF_TIMEOUT` (default 28800 s).
- `bin/run-all.sh` refreshes everything.
- Stage1 is rebuilt by the oracle when present; when the oracle is
  sunset, drop a self-produced stage2 at
  `.goalpost/artifacts/compiler2-stage1.smt2` and the harnesses use it
  (recorded in the artifact's `stage1_builder`).

## What is deliberately NOT measured

- compiler.smt2 / legacy selfhost-path test phases (`test.sh`) — they
  measure the *old* compiler; this goal is about compiler2.
- Byte-fixpoint of stage2/stage3 — the goal asks for a *working*
  artifact from itself, which `selfhost_stage2_works` operationalizes;
  a fixpoint gate can be added by amendment if the operator wants the
  stronger property.
- Actual deletion of the oracle/`compiler/` — the goal says
  *deletable*; deletability is exactly `selfhost_stage2_works` ∧
  `legacy_compiler_imports == 0`.

## Initial readings (2026-06-07, this machine)

`legacy_compiler_imports` = 3. Corpus artifacts were generated with a
reduced per-fixture cap (recorded in each artifact) to verify the
plumbing inside the authoring session; the canonical run is
`bin/run-all.sh` at default caps. The pass path was verified for real:
single-fixture probes at the 1800 s cap returned **pass** for
conformance fixtures `001-int-arithmetic-add` (smt2-contains + exit 7)
and `002-string-literal-print` (stdout + exit + smt2-contains), each
compiling in ~10–11 min through kernel+compiler2-stage1. Sample and
self-host compiles recorded honest `compile_timeout` at the reduced
caps. Targets were NOT tuned to current state.

Hashes left unlocked — human approval required before md runs these.

---

# Goalpost review — `compiler2-purism` (amendment 2026-06-10)

**Goal (restated):** `compiler2/*.ev` stops violating the language's
surface rules (`docs/evident-purism.md`): numbered-variable families
emulating collections (V18), the hand-peeled cons-list bind families
in `driver_compose.ev` and consumers, component-prefix namespacing of
fsm internals plus the `..`-lift compositions in `driver.ev` that
should be bare mentions, and value-selection/case-code ternary chains
(V9; carried-write hold chains are blessed) — each instance gone or
carrying a documented justification. Cryptic 1–3-char names (`st`,
`ty`, `nat`) become real words (§3.6).

## Architecture: live greps over compiler2/*.ev, critic as secondary

Every primary measure runs in milliseconds against the live tree —
no artifacts, no harnesses. Comments and string literals are stripped
before counting, so prose mentions and wire-format strings never
count as violations (and deleting a violation but leaving it named in
a MODULE header doesn't either).

**Survivor justification — the exemption ledger.** The goal allows
survivors "carrying a documented justification". The mechanical form
is one line per survivor in **`docs/purism-exemptions.md`** (does not
exist yet; absence = zero exemptions):

```
<CLASS> <file.ev|*> <token> — <reason>
e.g.  V18 driver_compose.ev bind_tail — cons-list carry gap (Seq-in-enum payload); critic v2 W-row
      V11 driver.ev DriverEmit — deliberate context sharing: <what is shared>
      V5 driver_group.ev group_* — <reason>
      V9 driver_window.ev win_need — <reason>
      naming * ty — `type` is a reserved keyword (driver_ir.ev)
```

Every measure reports its exempted count inside its label, so
mass-exemption is visible on the panel, not silent — and the ledger
itself is a reviewable, diffable artifact. (V18 exemptions match by
stem — a family spans files; the file field is audit metadata.)

## Measures

| measure | kind | rung | target | baseline (2026-06-10) | inspects | why a skeptic should believe it |
|---|---|---|---|---|---|---|
| `v18_numbered_families` | trend | det | 0 unexempted | **49** | stems with ≥3 trailing-digit identifiers (`dec_tok0..7`-shape) in comment-stripped `compiler2/*.ev` | Counts the actual scalar families in the actual source; threshold 3 skips incidental pairs; renaming `xs0→xs_a` to dodge it still shrinks the family below threshold only by actually removing it or by visible ledger exemption. |
| `v18_bind_peel_refs` | gate | det | 0 unexempted | **96** | occurrences of `bind_n[0-9]`/`bind_h[0-9]`/`bind_tail[0-9]` (the goal's named class) across compiler2 | Direct count of the named family's references — driver_compose.ev's peel plus every consumer; goes green only when the Seq-of-record replacement lands or the stems are ledger-justified. |
| `fsm_prefix_decls` | trend | det | 0 unexempted | **276** | decl-position names inside each `fsm DriverXyz` body that start with the fsm's own component word(s) or their concatenation + `_` (DriverGroup→`group_*`, DriverSetVar→`set_*`/`var_*`/`setvar_*`) | Derives the forbidden prefixes from the fsm's own name — no hand-kept list to game; catches exactly the "fsm namespacing its own internals" symptom; a rename to a *different* opaque prefix is caught by the critic (secondary) and the V5 ledger requirement. |
| `driver_lift_compositions` | gate | det | 0 unexempted | **24** | `..Name` composition lines in `compiler2/driver.ev` | The goal says these become header-based bare mentions except deliberate context-sharing lifts; a survivor needs a V11 ledger line naming what is shared. Dropping the `..` without converting to a bare mention changes program meaning and is caught by the conformance gate, so this can't be gamed by deletion. |
| `v9_selection_chains` | trend | det | 0 unexempted | **62** | statements (joined by paren balance, so multi-line chains count once) with ≥2 literal-equality ternary tests (`x = 3 ?` / `x = "key" ?`); chains whose final else-arm is a carry `_name` are the blessed hold form and excluded by construction | Implements the goal's own exemption (hold chains blessed) mechanically; matches the critic's V9 class within a few counts (62 vs 32 grouped findings). Splitting a chain across lines doesn't hide it; rewriting to keyed-projection pins genuinely removes it. |
| `cryptic_name_refs` | trend | det | 0 unexempted | **66** (`ty`=66, `st`=0, `nat`=0) | word-bounded uses of the goal's denylist (`st`, `ty`, `nat`) in comment/string-stripped compiler2 source | The denylist comes from the goal statement verbatim; `ty`'s existing "reserved keyword" rationale can survive only as an explicit `naming * ty` ledger line a human can see and veto. |
| `critic_v18_v9_findings` | trend | det | 0 | **55** (4 BLOCKER + 19 + 32 WARN) | latest (by `**Date:**`) `docs/critic-reports/*baseline*.md`: summary-table rows citing V18/V9, BLOCKER+WARN summed | The secondary signal the goal names: full-rulebook judgment cross-checking the greps. Ruler-broken (exit 2) if no report with a verdict line exists. |
| `critic_report_age_days` | gate | det | ≤14 d | **0** | date of that report | A stale critic report can't masquerade as current truth; the greps stay live regardless. |

**Definition of done** = both gates at 0 unexempted, the four primary
trends at 0 unexempted, and the critic secondary at 0 with a ≤14-day
report — i.e. every instance of the four classes is gone or visibly
ledger-justified, and an independent full-rulebook review agrees.

## Limits stated honestly

- `fsm_prefix_decls` derives prefixes from fsm names, so an
  *abbreviated* prefix (`win_` in DriverWindow) or an unrelated
  letter-code prefix is not caught here — that class lands in the
  critic secondary (V5/V14 rows are not part of the V18/V9 sum, so
  full coverage of abbreviations rides on critic review + ledger).
- `v9_selection_chains` requires literal-equality tests; a chain
  dispatching on `matches` or comparisons is not counted (none exist
  in the baseline scan's blind spot today per the critic report).
- All scripts: <50 ms each, self-time-boxed at 55 s, read-only,
  cwd-independent, deterministic (verified by double-run diff).

Hashes left unlocked — human approval required before md runs these
(reviewer: also note `code_quality.sh` predates this amendment and has
no REVIEW/CHANGELOG entry; flagged in CHANGELOG, not touched).
