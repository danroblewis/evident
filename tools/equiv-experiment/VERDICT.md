# Translation-validation feasibility verdict

**Question.** Evident programs compile to one SMT formula, so in principle Z3
can *prove* an edit preserves behavior for ALL inputs (translation validation),
instead of testing it on fixtures. Does this scale on the **whole self-hosted
compiler** (`stage1.smt2`), or choke?

**Short answer.** The naive **semantic** approach — one Z3 query over both whole
bodies — **does NOT scale**: it times out at 600 s on a clean bijective rename.
But for the refactor class these tools target (rename / carried-record
restructure, where φ is a bijection and the bodies are identical modulo φ), a
**syntactic α-equivalence check proves equivalence in ~0.02 s** — and gives a
*stronger* guarantee than the single-tick Z3 query. So whole-compiler
translation validation is **viable as a syntactic tool today**, and remains a
research project as a *semantic* (Z3) tool.

All numbers: dev box, Z3 4.15.4.

## Scale of one `stage1` emit

| | value |
| --- | --- |
| emitted lines (`stage1.smt2`) | 25,459 |
| top-level `declare-fun` (0-arity consts) | 5,932 |
| top-level `assert` | 3,360 |
| manifest carried state-fields | 2,669 |

The **semantic** equivalence query asserts BOTH bodies → ~9,300 OLD statements +
5,932 renamed NEW consts, 2,672 input bridges, 2,686 observable-output
disjuncts, ~56,000 lines.

## Results

### A. Semantic Z3 query — qloop pair (`b955bdd`, qloop_ → QLoop record)

φ: clean 14-element bijection (7 fields × {base, `_`dual}), auto-derived, 0
unmatched.

| metric | value |
| --- | --- |
| z3 result | **timeout** (no answer) |
| budget / wall | 600 s / 600.19 s |
| z3 `:time` | 599.95 s |
| rlimit-count | 103,699,520 |
| memory | 7,602 MB |
| decisions / conflicts | 767,493 / 1,583 |

The decisions-to-conflicts ratio (767k : 1.6k) shows Z3 thrashing in search,
not converging on the proof. The model mixes `String` theory, recursive
datatypes (`Effect`, cons-list `Seq`), and 2,686 array/record output
equalities — exactly the combination Z3 is weakest at proving *equal*.

**Tactic retry:** `(check-sat-using (then simplify propagate-values solve-eqs
simplify smt))` — also **timeout** at 180 s (rlimit 64,236,067). No off-the-shelf
tactic rescued it.

### B. Syntactic α-equivalence — both real commits

`build-equiv-query --syntactic`: phi-normalize the NEW emit (token-accurate,
new→old) and compare the `declare-fun`+`assert`+datatype STATEMENT SETS.

| commit | restructure | φ size | statements | result | wall |
| --- | --- | --- | --- | --- | --- |
| `b955bdd` | qloop_ → QLoop record | 14 | 9,316 | **equivalent** | 0.020 s |
| `30c3eda` | parse_ → ParseState record (17 files) | 22 | 9,316 | **equivalent** | 0.019 s |

Both real refactor commits are, at the SMT level, **pure α-renames**: every
`declare-fun` and `assert` matches byte-for-byte after applying φ. The syntactic
check proves this in ~20 ms — **~30,000× faster** than the (timed-out) 600 s
semantic query, and *more* conclusive (exact structural identity, no single-tick
caveat).

## Soundness controls (these MUST hold or no number means anything)

Semantic builder, on a tiny single-`fsm` emit:

| control | query | result | meaning |
| --- | --- | --- | --- |
| identity | program vs. itself, φ=∅ | **unsat** | identical ⇒ proven equivalent ✓ |
| divergence | `n=5`-trigger vs `n=6`-trigger | **sat** | a real behavior change ⇒ witness found ✓ |

The identity control is load-bearing: the *first* (whole-array) construction
returned **sat** for a program against itself — `effects` is an `(Array Int
Effect)` unconstrained past `effects__len`, so two copies disagree on garbage
indices. Comparing `effects` **observably** (element-wise to `max-effects`,
guarded by the shared length) fixed it to `unsat`. A validator that skips this is
silently unsound.

Syntactic checker negative controls:

| control | result | meaning |
| --- | --- | --- |
| two *different* compiler versions, φ=∅ | **differs** (97/97 residue) | a real delta is NOT called equivalent ✓ |
| qloop pair with *empty* φ | **differs** (31/31, all qloop lines) | a missing φ mapping is caught, not hidden ✓ |

## Verdict

**Whole-compiler translation validation is a viable tool — but as a SYNTACTIC
α-equivalence checker, not a semantic Z3 prover.**

- For the **rename / carried-record-restructure** class (the bulk of the
  de-prefixing refactors this tooling exists for), the syntactic check is fast
  (~20 ms), exact, and stronger than the semantic query. It should be the
  default gate for these commits — and it scales trivially because it never
  invokes a solver.
- The **semantic Z3 query does NOT scale** to the whole compiler: it times out
  at 600 s on the same trivially-true rename, defeated by the String + datatype
  + thousands-of-array-equalities combination. As a *general* translation
  validator (for commits that change *logic*, not just names — where syntactic
  equality won't hold) it remains a **research project**: it would need
  per-subsystem scoping, theory-specific tactics, or an inductive/abstraction
  layer, none of which an off-the-shelf `(check-sat)` provides.
- A natural middle path (subsystem scoping) is **blocked by the oracle
  contract**: a sub-FSM like `DriverQuant` can't be emitted standalone
  (`emit` requires exactly one `effects` writer, which only `driver_main`
  assembles), so you can't cheaply Z3-check just the changed subsystem. Whole
  programs only.

**Recommendation.** Ship `--syntactic` as the equivalence gate for
rename/restructure commits (it is sound, fast, and exact). Keep the semantic
builder as a documented prototype + the soundness harness; do not put it on any
hot path. When a future commit genuinely changes logic, fall back to `evt diff`
(concrete old-vs-new on fixtures) and conformance — the semantic prover is not
ready to replace them.

## What the syntactic check proves vs. assumes

- **Proves:** the two emits are identical modulo φ (every declare/assert matches
  exactly). That is *stronger* than single-tick output equivalence — it is whole
  *formula* equivalence, hence equivalence on every tick of every input.
- **Assumes:** φ is correct. `build-phi.sh` derives φ structurally from the
  declare-fun set diff (pair old-only↔new-only by `(is_dual, field)` key) and
  **refuses a non-bijection** (unequal set sizes / unpaired residue print a loud
  warning). A wrong φ shows up as residual differing statements, not a false
  "equivalent" — the negative controls confirm this.
- **Scope:** only meaningful when the *entire* delta is φ. If a commit changes
  logic, the statement sets won't match and the tool correctly reports the
  residue (refusing to claim equivalence) — at which point use `evt diff` +
  conformance.

## What the semantic query proves vs. assumes

See `INDUCTIVE.md`. It is single-tick OUTPUT equivalence under φ (necessary, not
sufficient; for a bijective rename it is effectively the inductive step). It is
the honest *semantic* notion — but per the timing above it does not scale, so
it is a documented prototype, not a shipped gate.

## Reproduce

```sh
cd tools/equiv-experiment/build-equiv-query && cargo build --release && cd -

# OLD/NEW stage1 emits for a commit pair (flatten | evident-oracle emit driver_main),
# then phi + syntactic check + (optional) timed semantic z3 — all via:
tools/equiv-experiment/run-experiment.sh b955bdd      # qloop
tools/equiv-experiment/run-experiment.sh 30c3eda      # ParseState

# Just the fast syntactic gate on two existing emits:
build-equiv-query --syntactic OLD.smt2 NEW.smt2 phi.txt
```
