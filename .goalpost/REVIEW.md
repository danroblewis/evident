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
