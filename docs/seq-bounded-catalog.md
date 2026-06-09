# Bounded-`Seq` construction catalog

A verified catalog of every `Seq` construction Evident programs reach
for, **all on bounded sequences** (a static literal length or `#s ≤ N`).

## Thesis

The Z3 sequence *theory* is semi-decidable and risky **only when a
sequence is unbounded**. Once a `Seq` has a static length bound, every
construction below lowers to cheap, decidable **Array + length /
bounded-quantifier** form. There is no "slow operation" — only "is it
bounded." This document verifies that claim construction by
construction, twice over:

1. **Z3 ground truth** (`tests/seq/z3/*.smt2`): the bounded encoding
   (uninterpreted `Int → Int` array + a static length / bounded
   quantifier), asserting a positive case is `sat` and a negative case
   is `unsat`, with **no `unknown`/timeout**.
2. **Evident regression** (`tests/seq/*.ev`): a kernel-runnable fixture
   on a bounded `Seq` that exercises the construction through the real
   producing path (`flatten → evident-oracle emit → kernel`). SAT →
   `Exit(0)`; UNSAT → the tick solve fails → exit 2.

**Verdict: confirmed.** All 18 Z3 encodings return decidable `sat`/`unsat`
(whole suite ~0.14s). All 39 Evident fixtures pass (whole suite ~1.2s);
the heaviest (toposort permutation, contains-with-free-offset) solve in
**1–3 ms of Z3 time each** (per the `[functionizer]` line). Nothing is
slow. The only surprises are *expressiveness* gaps in today's oracle, not
decidability or performance gaps — see the matrix.

## Support matrix

Legend — **Z3**: bounded encoding verified sat+unsat. **Evident**: does
it compile through the oracle today? `yes` / `GAP`. **Fixtures**: the
files that pin it.

| # | Construction | Z3 | Evident today | Fixtures |
|---|--------------|----|---------------|----------|
| A1 | ordering chain `⟨a,b,c⟩ ⇒ a<b<c` | sat/unsat | yes | `01_ordering_chain_{sat,unsat}.ev` · `z3/01` |
| A2 | overlapping-chain merge (+ conflicting overlap UNSAT) | sat/unsat | yes | `02_merge_sat.ev` · `02_merge_conflict_unsat.ev` · `z3/02` |
| A3 | toposort (pos perm; `pos[from]<pos[to]`) | sat/unsat | yes — **but edge endpoints via coindexed parallel seqs, not a `Seq` of records** (record-field access on a Seq element is a GAP, see below) | `03_toposort_{sat,cycle_unsat}.ev` · `z3/03` |
| B4 | literal / empty `⟨⟩` | sat/sat/unsat | yes | `04_literal_empty_{sat,unsat}.ev` · `z3/04` |
| B5 | concat `a ++ b ++ ⟨x⟩ ++ c` order | sat/unsat | yes | `05_concat_{sat,unsat}.ev` · `z3/05` |
| C6 | length `#xs` | sat/unsat | yes | `06_length_{sat,unsat}.ev` · `z3/06` |
| C7 | indexing `xs[i]` | sat/unsat | yes | `07_index_{sat,unsat}.ev` · `z3/07` |
| C8 | universal `∀ x∈xs` / existential `∃ x∈xs` | sat/unsat·sat/unsat | yes | `08_universal_{sat,unsat}.ev` · `08_existential_{sat,unsat}.ev` · `z3/08` |
| C9 | coindexed/zip `∀ (a,b)∈coindexed(xs,ys)` | sat/unsat | **yes — parses AND constrains** (verified by the unsat case) | `09_coindexed_{sat,unsat}.ev` · `z3/09` |
| C10 | consecutive pairs `∀ (a,b)∈edges(xs)` | sat/unsat | **yes — parses AND constrains** (verified by the unsat case) | `10_edges_{sat,unsat}.ev` · `z3/10` |
| C11 | membership `x ∈ xs` | sat/unsat | **GAP — direct `∈` on a Seq is silently DROPPED.** Supported equivalent: `∃ i∈{0..#xs-1} : xs[i]=x` | `11_membership_exists_{sat,unsat}.ev` (supported) · `11_membership_direct_GAP.ev` (gap witness) · `z3/11` |
| D12 | sortedness `∀ i : xs[i]≤xs[i+1]` | sat/unsat | yes | `12_sorted_{sat,unsat}.ev` · `z3/12` |
| D13 | permutation / all-distinct `distinct(xs)` | sat/unsat | yes | `13_distinct_{sat,unsat}.ev` · `z3/13` |
| D14 | subset `∀ x∈xs : x∈s` | sat/unsat | **yes via `Set` membership** (note: `x ∈ Set` compiles even though `x ∈ Seq` does not) | `14_subset_{sat,unsat}.ev` · `z3/14` |
| E15 | prefix `plen≤slen ∧ ∀ i<plen : p[i]=s[i]` | sat/unsat | yes | `15_prefix_{sat,unsat}.ev` · `z3/15` |
| E16 | suffix `p[i]=s[slen-plen+i]` | sat/unsat | yes | `16_suffix_{sat,unsat}.ev` · `z3/16` |
| E17 | contains (contiguous) `∃ off : ∀ j<sublen : s[off+j]=sub[j]` | sat/unsat | yes (`off` a bounded free Int) | `17_contains_{sat,unsat}.ev` · `z3/17` |
| E18 | fixed-window extract `w[j]=s[start+j]` | sat/unsat | yes | `18_window_{sat,unsat}.ev` · `z3/18` |

## What Evident supports on a bounded Seq TODAY vs the gaps

**Supported (15 of 18 constructions compile directly):** literal/empty,
`#`, `xs[i]`, `++`, `∀ x∈xs`, `∃ x∈xs`, `coindexed`, `edges`, `distinct`,
sortedness, prefix/suffix/contains/window (all bounded-quantifier index
forms), Set-membership subset, ordering chains and chain merges.

Two pleasant surprises confirmed by their **unsat** fixtures (not just
"it didn't error"): `coindexed(...)` and `edges(...)` both parse *and*
emit real constraints. The brief flagged them as uncertain; they work.

**Gaps (real, recorded precisely):**

1. **`x ∈ xs` (Seq membership) is silently dropped.** The oracle cannot
   translate Seq membership to a Z3 Bool, so the constraint vanishes —
   `x` does not even appear in the emitted SMT2, and the claim becomes
   vacuously SAT. Witnessed by `11_membership_direct_GAP.ev` (target=99
   is absent yet the fixture exits 0). The decidable, supported
   replacement is `∃ i∈{0..#xs-1} : xs[i]=x`. Note the gap is specific to
   **Seq**: `x ∈ Set(T)` membership compiles correctly (see D14).

2. **Record-field access on a `Seq` element does not compile.** For a
   `type Edge(from, to ∈ Int)` and `edges ∈ Seq(Edge)`, expressions like
   `edges[0].from`, `e.from` inside `∀ e ∈ edges`, and `pos[e.from]` all
   emit "dropped constraint (couldn't translate to Bool)". This blocks
   the textbook record-typed toposort `∀ e ∈ edges : pos[e.from] <
   pos[e.to]`. Workaround used in A3: carry edge endpoints as two
   coindexed `Seq(Int)` (`efrom`/`eto`) and bind them with
   `∀ (u,v) ∈ coindexed(efrom, eto) : pos[u] < pos[v]`. (This is the one
   place the catalog deliberately uses parallel seqs — coindexed binds
   them, so it is not the "silent misalignment" footgun CLAUDE.md warns
   against. The clean record-typed form is the desired end state once the
   translator gains Seq-element field access.)

3. **`Edge<Int>(0,2)` generic-record literal in expression position
   fails to parse** (`expected expression, got RParen`). Separate from
   gap 2; noted while probing the stdlib `Edge<T>` path. Non-generic
   record literals in `⟨…⟩` are fine.

None of these are decidability or performance problems — they are
translator-coverage gaps. The thesis (bounded ⇒ decidable & cheap) holds
for every construction; where Evident can express the construction at all,
it is fast.

## How to run

```sh
tests/seq/z3/run.sh     # 18 Z3 scripts; checks sat/unsat sequence + no 'unknown'
tests/seq/run.sh        # 39 Evident fixtures through the real producing path
```

Both exit 0 iff everything passes. `tests/seq/run.sh` resolves the kernel
from the main checkout (`EVIDENT_KERNEL` overridable); this worktree
carries no `kernel/target`.

## Files

- `docs/seq-bounded-catalog.md` — this catalog.
- `tests/seq/run.sh` — Evident fixture driver.
- `tests/seq/*.ev` — 39 SAT/UNSAT fixtures (one or two per construction).
- `tests/seq/z3/run.sh` — Z3 verification driver.
- `tests/seq/z3/*.smt2` — 18 bounded-encoding verification scripts.
