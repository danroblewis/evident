# Toposort<Int> baseline — 2026-05-16

Captured against `stdlib/toposort.ev` at commit `c01083d` (Phase 6.3
groundwork), release build, M-series macOS. Each cell is one
`rt.query("Toposort<Int>", given)` call against a freshly-loaded
runtime. `load` is parsing stdlib/toposort.ev + its imports; `solve`
is translation + Z3 solve + extract.

Reproduce:
```
cd runtime
cargo run --release --example bench_toposort
# or with size cap:
MAX_N=32 cargo run --release --example bench_toposort
```

## Numbers

| shape         |   n |   edges |   load |    solve | result |
|---------------|----:|--------:|-------:|---------:|:------:|
| empty         |   4 |       0 |  3.0ms |    3.8ms | SAT    |
| empty         |   8 |       0 |  2.3ms |    1.7ms | SAT    |
| empty         |  16 |       0 |  2.4ms |    2.8ms | SAT    |
| empty         |  32 |       0 |  2.2ms |    7.0ms | SAT    |
| empty         |  64 |       0 |  ~2ms  |   13.4s  | SAT    |
| linear_chain  |   4 |       3 |  8.6ms |    1.7ms | SAT    |
| linear_chain  |   8 |       7 |  4.6ms |    4.4ms | SAT    |
| linear_chain  |  16 |      15 |  2.1ms |   23.2ms | SAT    |
| linear_chain  |  32 |      31 |  2.2ms |    199ms | SAT    |
| linear_chain  |  64 |      63 |  ~2ms  |   15.9s  | SAT    |
| fanout_tree   |   4 |       3 |  1.8ms |    1.4ms | SAT    |
| fanout_tree   |   8 |       7 |  1.7ms |    4.3ms | SAT    |
| fanout_tree   |  16 |      15 |  1.9ms |   41.3ms | SAT    |
| fanout_tree   |  32 |      31 |  4.7ms |    520ms | SAT    |
| fanout_tree   |  64 |      63 |  —     |   >90s   | (DNF)  |
| dense_dag     |   4 |       3 |  2.5ms |    2.1ms | SAT    |
| dense_dag     |   8 |      13 |  2.4ms |    7.9ms | SAT    |
| dense_dag     |  16 |      54 |  2.4ms |   46.6ms | SAT    |
| dense_dag     |  32 |     220 |  2.5ms |   1.10s  | SAT    |
| dense_dag     |  64 |     888 |  2.3ms |   43.4s  | SAT    |

Reference: a native Rust Kahn's-algorithm toposort on the n=64
dense_dag graph runs in **single-digit microseconds**. The gap to
the Evident-on-Z3 implementation at n=64 is ~10⁶–10⁷×.

## Shape of the curve

Solve time doubling at each step (extrapolated where DNF):

| shape         | 4→8 | 8→16 | 16→32 | 32→64 |
|---------------|----:|-----:|------:|------:|
| empty         | 0.5 |  1.6 |   2.5 | ~1900 |
| linear_chain  | 2.6 |  5.3 |   8.6 |    80 |
| fanout_tree   | 3.1 |  9.6 |  12.6 |  >170 |
| dense_dag     | 2.8 |  5.9 |  23.6 |    40 |

The "empty" cliff (32 → 64 is ~1900×) is the most striking finding.
The permutation/distinct/membership encoding has a phase change
inside Z3 between 32 and 64 elements that's far steeper than any
of the edge-bearing variants. The other shapes scale roughly as
~10× per 2× n in the n≤32 regime and ~100× at the 64 cliff.

## Where the cost is coming from (hypotheses, untested)

1. **`distinct(p)`** in `Permutation<T>` expands to O(n²) pairwise
   `p[i] ≠ p[j]` constraints today. At n=64 that's 2016 disequalities.
   Should swap to Z3's native `distinct(...)` solver builtin if we
   aren't already.
2. **`∀ x ∈ p : x ∈ s`** expands to `n` Set memberships, each of
   which lowers to an n-way disjunction. O(n²) total disjuncts.
3. **`position_of(sorted, x)`** is a chained-ITE of depth n, called
   twice per edge. For dense_dag at n=64 with 888 edges, that's
   ~1.1M ITE branches in the translated formula.
4. **Z3 search-space cliff** — with no structural symmetry-break,
   Z3's default tactic enumerates over n! permutations at the
   leaf. The 32→64 cliff for the empty case (no ordering
   constraints to prune) is the signature of that.

## Two paths forward

### A. Self-hosted Evident-side rewrite

Same constraint model, better encoding:

- Replace `position_of(sorted, x)` (an O(n) chained ITE) with an
  explicit `positions ∈ Seq(Int)` that's the inverse: edges become
  `positions[edge.from_idx] < positions[edge.to_idx]`. Edge cost
  drops from O(n) per edge to O(1). Then `sorted` is recovered from
  positions in extraction, or left as the secondary output.
- Use Z3's native distinct on the position vector instead of
  whatever lowering `distinct(p)` is producing.

These are pure encoding changes — Z3 still does the search, just
on a more compact formula. Expected: linear/fanout/dense should
clear n=64 in the 100ms range; empty's cliff probably persists.

### B. Compile to native

The toposort claim has a recognizable shape: "topological sort of
a Seq(Edge<T>) over Set(T)". If the runtime detects this shape at
load time, it can emit a Rust toposort function and bypass Z3.
The native sort returns an ordering in microseconds; the runtime
calls it as a primitive, returning the same `Value::SeqInt` it
would have extracted from a model.

The general principle the user articulated: many of our constraint
claims have a function-shape — given inputs, there's a unique
output (or one is chosen if multiple satisfy). For those, we can
toposort the dependency graph between variables and emit code
instead of constraints. Toposort is the obvious first candidate
because (a) it's already used by the runtime internally and (b)
the function-version is a textbook 10-line algorithm.

The decision: which path first? A is incremental and stays inside
the Evident self-hosting story. B is the bigger win but the
bigger build. Probably A first to lock in a baseline that's
"good Evident encoding", then B to leapfrog the encoding entirely.
