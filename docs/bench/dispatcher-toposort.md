# Dispatcher Toposort: Rust vs. Evident — 2026-05-16

## The actual graph

The Rust-side `topo_sort_with_random_tiebreak` in
`runtime/src/trampoline.rs` orders Effects within a tick. For Mario
(`examples/test_21_mario/main.ev`) the graph at dispatch time is:

```
26 nodes   (effect-typed bindings + Seq(Effect) bundle elements)
33 edges   (intra-bundle auto-edges + the phase_chain explicit chain)
```

This is materially smaller than the toposort bench at n=32 — the
bench's "dense_dag n=32" had 220 edges. Mario's edge count is closer
to "linear_chain n=32 + a bit".

## Head-to-head

```
EVIDENT_DISPATCH_TIMING=1 evident effect-run examples/test_21_mario/main.ev
```

| impl                                   | nodes | edges | tick-0 time |
|----------------------------------------|------:|------:|------------:|
| Rust (`topo_sort_with_random_tiebreak`)|    26 |    33 |   **0.010 ms** |
| Evident (`rt.query("Toposort<String>", …)`) |    26 |    33 |   **521 ms** |

**~52,000× slowdown.**

Caveat: only one toposort per FSM per run, cached for the rest of
the program lifetime via `DISPATCH_ORDER_CACHE` (same shape →
HashMap lookup). Mario's effect graph is shape-stable across ticks,
so this is a one-time startup cost. For a 60fps game it's tolerable
but visible as a ~500ms hitch at frame 0; for development iteration
it's annoying; for any program where the graph shape changes per
tick it would be a hard wall.

The original abandonment claim from commit `b2c7bf2` was 12–16s in
the same Z3 context. The current 521ms is ~25× better than that —
presumably from incidental runtime improvements between then and
now. Still ~50,000× off Rust.

## How to switch

```
EVIDENT_TOPOSORT_IMPL=rust       # default
EVIDENT_TOPOSORT_IMPL=evident    # dogfood path
```

Implementation:
- Dispatcher branches in `runtime/src/trampoline.rs` on
  `EVIDENT_TOPOSORT_IMPL`. Evident path calls `evident_toposort()`
  which builds the `given` from the node/edge lists and calls
  `rt.query("Toposort<String>", …)`.
- Mario (and any future demo wanting the flag) must
  `import "stdlib/toposort.ev"` so the schema is loaded.
- `stdlib/toposort.ev` ends with a `sat_toposort_string_mono` claim
  that pins `Toposort<String>`'s monomorphization at load time.

## What this implies

The "dogfood our own primitive" goal is sound — the call works,
returns correct orderings, integrates cleanly. The blocker is
purely speed. The path to fixing it is unchanged from the toposort
baseline doc:

* **A. Better Evident-side encoding** — replace `position_of` (depth-n
  chained ITE, called twice per edge) with an explicit `positions ∈
  Seq(Int)` parallel to `items`. Edge cost drops O(n) → O(1).
  Expected: bring 521ms down to single-digit ms range.
* **B. Compile the model** — recognize the toposort shape at load
  time, emit a Rust toposort, bypass Z3 entirely. Expected: identical
  to the current Rust path.

(A) is the Evident-self-hosting win; (B) is the general
"compile-claims-to-functions" win. The flag stays as a comparison
tool through both efforts.
