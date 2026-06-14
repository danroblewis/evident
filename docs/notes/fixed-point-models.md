# Fixed-point models — a noted future direction (not built yet)

Two ways we expect to make recursive/iterative models fast:

1. **Tail recursion** — a transition relation unrolled, with memory reuse
   (one set of state slots, overwritten each step). Prototyped in
   `prototype/models/` (`run_incremental` = the memory-reuse strategy).

2. **Fixed-point models** — the interesting, weird one. Some transition-function
   models are really computing a **fixed point**: iterate `state → step(state)`
   until `step(state) = state`. When that's the case, the fixed-point form is
   usually *clearer* and *makes more sense* than the hand-written transition —
   and it can be much faster (Z3 can sometimes solve `∃s. s = step(s)` directly,
   or with far less unrolling than a fixed depth bound).

The asymmetry that makes this worth tooling:

- If a transition model **is** a fixed point, the programmer might still prefer
  to write the readable transition form.
- So: **analysis tooling that detects fixed-point structure** and either
  (a) *suggests* the programmer rewrite it as a fixed-point model, or
  (b) *automatically rewrites* it to the fixed-point form at compile/run time —
  getting the speedup without forcing the less-readable source on anyone.

Auto-detection is the high-leverage bit: a transition where the next state is a
pure function of the current state, with a reachable `step(s) = s`, is a
candidate. Detecting it lets us keep the readable surface and still take the
fast path — the same "keep the surface, change the lowering" principle the
benchmark suite established, applied to recursion.

Status: idea only. The `prototype/models/` POC builds the transition + unroller
(both execution strategies) and the prettified per-sub-model reports; fixed-point
detection/rewrite is a later layer on top.
