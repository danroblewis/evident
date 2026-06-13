# prototype — Z3 cross-theory benchmark suite

Branch `prototype-z3-python`. We stripped the `Evident → smt2 → Z3 → kernel`
stack down and are prototyping the bottom layer directly in **Python over Z3**,
to answer one question empirically: *for a given problem, which Z3 theory and
which tactic pipeline actually solve it fastest?*

The thesis behind the Evident rewrite is "model with sets / relations, then let
a tactic lower the slow encoding." This suite measures whether that holds —
every task is written in several semantically-identical encodings (set, array,
arithmetic, bitvector, …) and run under every tactic combination, so the
fastest path is *discovered*, not assumed.

## Layout

```
prototype/
  run.py                  CLI entry point (run / report / profile / list)
  benchsuite/             the suite, as a package
    tasks.py              task registry: problems × semantically-equal encodings
    tactics.py            tactic catalogue + the combinatorial sequence generator
    harness.py            timing core (wall-clock floor + Z3 rlimit work counter)
    runner.py             the sweep: task × scale × encoding × tactic-sequence
    profiling.py          model AST fingerprint + before/after diff
    report.py             CSV / JSON / markdown / model-diff / text-summary
  results/                generated artifacts (run.csv/.md/.json, *-modeldiff.md)
  z3-capabilities.md      reference: every theory, sort, predicate Z3 exposes
  set-lowering-via-z3.md  the blast_select_store finding, with Z3 source refs
  FINDINGS.md             cross-theory results + conclusions
```

## Run

```bash
python3 run.py list                          # tasks, theories, tactics, counts
python3 run.py run --max-len 2               # the sweep → results/run.{csv,md}
python3 run.py run --tasks dispatch coloring --max-len 3 --reps 3
python3 run.py report results/run.csv --markdown results/run.md
python3 run.py report results/run.csv --model-diff results/run-modeldiff.md
python3 run.py report results/run.csv --translations results/translations  # all smt2
python3 run.py report results/run.csv --translations results/set-theory.md \
        --theory set --single-file --cap 1200          # focused, one self-contained .md
python3 run.py profile dispatch set 200 --tactics blast   # AST diff under a tactic
```

`run` writes a CSV, a markdown report (`run.md`), a per-encoding **model-diff**
(`run-modeldiff.md`), and a text summary to stderr (soundness and timeout
canaries included). `--json` adds a JSON dump. `report` regenerates any of these
derived outputs from an existing CSV without re-solving.

### The reports

- **`run.md`** — *timing*: each encoding ranked by baseline solve time, with the
  fastest tactic sequence found.
- **`run-modeldiff.md`** — *structure, winners only*: for each encoding's winning
  sequence, how the model changed (Δ DAG nodes, Δ distinct symbols, and the
  operation counts that moved most — e.g. `store 200→0` where
  `blast_select_store` blasts a store-chain away).
- **`translations/`** — *every translation*: `--translations DIR` dumps the
  before/after smt2 for **every** case (deduped — sequences that yield an
  identical model share a file) under `DIR/smt2/`, and writes `DIR/index.md`:
  per (task, encoding, scale) group, every tactic sequence ranked by total ms
  with a link to its model, plus a rendered ` ```diff ` of baseline→model for
  each distinct translation (large models link out instead of inlining).
- **`set-theory.md`** — *one theory, self-contained*: `--theory set --single-file`
  restricts the translations report to the set-theory encodings (`dispatch/set`,
  `dispatch/set_bv`, `reachability/unroll_set`) and inlines every tactic-run diff
  into a single `.md` (no smt2 tree). This is where the set→ite lowering is most
  visible — each set-membership store-chain collapses to `(goal)` under
  `blast_select_store`. Any theory works (`--theory array`, `--theory bitvec`, …).
- **`profile`** — the same AST diff as model-diff, on demand, for any one case.

## The combinatorial sweep

For each **task**, each **scale**, each **encoding**, the runner applies every
tactic **sequence** and times the solve separately from the tactic application
(so a tactic's cost is never hidden inside the solve number).

The sequence set is the empty baseline plus every ordered sequence with
repetition of length `1..max_len` over the catalogue — each tactic alone, each
tactic twice, each after every other, and so on. With 7 tactics:

| max_len | sequences/case |
|--:|--:|
| 1 | 8 |
| 2 | 57 |
| 3 | 400 |
| … | … |
| 7 | 960 800 (the full "until we run out of tactics" sweep) |

`--max-len 2` is the practical default; the winners (notably
`simplify[blast_select_store=True]`) already surface there.

## Tasks

Each task is one problem with several encodings that must agree on sat/unsat;
the runner flags any tactic that changes the answer as a **soundness
violation**.

| task | what it is | encodings (theories) |
|---|---|---|
| `dispatch` | invert a scrambled map | arith, ite, array, func, set, set_bv |
| `coloring` | 3-colour a planted graph | int(≠), bitvec, onehot, enum |
| `reachability` | is target reachable | unroll_bool, unroll_set, special(TC) |
| `arith_system` | ordered vars summing to target | int, real, real_nl, bitvec |
| `string_match` | length-L string w/ "ab", ends "z" | string, regex |
| `seq_build` | bounded Seq(Int) containing 7 | seq |
| `fp_solve` | positive non-NaN Float32s | fp |

Together they exercise 14 theories. `python3 run.py list` prints the live set.

## Reading the output

The markdown report ranks each encoding by no-tactic baseline and shows the
fastest tactic sequence found. The headline results are in `FINDINGS.md`; the
load-bearing one: a set-of-tuples dispatch is ~1000× slower than an ite chain,
but `simplify` with `blast_select_store=True` rewrites the store-chain select
into that ite and recovers the gap (~700×) — the "safe lowering as a tactic"
the thesis predicted.
