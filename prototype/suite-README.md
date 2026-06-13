# Combinatorial benchmark suite

Automates: **one problem, encoded many ways, under every tactic sequence**, so we
can compare theories, encodings, multi-theory mixes, and tactic pipelines on equal
footing.

## Pieces

- **`suite_tasks.py`** — the task registry. Each task has several encodings; each
  encoding is `build(N) -> z3.Goal`, tagged with the theory/theories it uses
  (single- or multi-theory), plus its expected result. Encodings reuse the
  validated builds in `b01_dispatch` / `b02_coloring` / `b03_reachability` (one
  source of truth). Add a task or encoding here.
- **`suite.py`** — the runner. For each `task × scale × encoding × tactic-sequence`
  it applies the sequence (timed once), solves (reps, min wall), checks the result
  against expected, and streams a CSV row. `--summarize` reads the CSV back.

## The combinatorial axes

1. **theory** — pivot the CSV on the `theories` column.
2. **several encodings within a theory** — multiple encodings can share a tag.
3. **multi-theory encodings** — e.g. `dispatch/set_bv` is `set+tuple+bitvec`.
4. **tactic sequences** — *every* ordered sequence of the `TACTICS` list, with
   repetition, of length `1..max_len`, plus the empty baseline: each tactic alone,
   each doubled, each pair, then length 3, 4, … The full *"until we run out of
   tactics"* sweep is `max_len = len(TACTICS)`.

## ⚠ The size explosion

Sequence count per case = `1 + Σ_{k=1..L} T^k` for `T` tactics, length `L`:

| L | sequences (T=6) |
|---|---|
| 1 | 7 |
| 2 | 43 |
| 3 | 259 |
| 4 | 1 555 |
| 5 | 9 331 |
| **6 (full)** | **55 987** |

Times `(encodings × scales)` (~24) times `reps`. `L=2` is ~1 k solves (minutes);
the **full `L=6` sweep is ~1.3 M solves** — an overnight/batched job. The runner
streams to CSV and flushes per encoding, so a long run is partially-usable and
restartable by task. Scale up with `--max-len`, `--tasks`, fewer `--scales`
(edit the registry), and a tighter `--timeout`.

## Run

```
python3 suite.py --max-len 2                      # default: all tasks, ~1k solves
python3 suite.py --max-len 3 --tasks dispatch     # one task, deeper sequences
python3 suite.py --summarize suite_results.csv    # analyze any prior CSV
```

CSV columns: `task, scale, encoding, theories, combo, combo_len, result, ok,
tactic_ms, solve_ms, total_ms, size_before, size_after`. `ok` is the soundness
canary — a tactic must never change sat/unsat.

## Adding to the suite

- **A theory we don't cover yet** (Real/LRA, Seq, String, FP, …): add a task whose
  encodings exercise it, or add an encoding tag to an existing task.
- **A new task** (b04 arithmetic systems, b05 parse, b06 cardinality, b07 mixing):
  add validated builds (mirror the `b0X_*.py` style) and register them.
- **More tactics**: extend `TACTICS` — note it raises the explosion base `T`.
