# runtime/src/runtime/autotune.rs — Z3-replaceability
**What it does:** Implements a bandit-style auto-tuner for Z3's `smt.arith.solver` parameter. Cycles through candidate values (2=Simplex, 6=newer default), measures mean solve time over a pricing window of 30 frames per candidate, then locks to the winner. State machine: `Pricing{idx}` → `Locked{config}`.
**Criticality:** peripheral (load-time warmup; only affects the first ~60 ticks per schema until locked)
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** This is a runtime-performance heuristic that configures Z3's internals — it chooses which solver to pass to Z3, not a property that Z3 could decide. The tuner is a bookkeeping state machine over wall-clock timing data. There is no constraint-satisfaction problem here; the "best config" is an empirical measurement, not something expressible as a Z3 constraint. Expressing "which arith solver is faster on this workload" as a Z3 solve would be circular and meaningless.
**Change made:** none
