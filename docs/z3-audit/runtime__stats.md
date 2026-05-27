# runtime/src/runtime/stats.rs — Z3-replaceability
**What it does:** Defines `FunctionizeStats` and `PerClaimStats` — plain Rust structs that accumulate per-claim JIT/functionizer counters (analyses, cache hits, value-cache hits, simplified assertions, steps, components, etc.) and implement a `print_summary` method for the `effect-run` timing report.
**Criticality:** peripheral (diagnostic/observability only; no effect on correctness)
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** Pure data definitions + a formatted-print routine. There is no algorithm, no decision, no property to check — it is an observation accumulator. Not a constraint-satisfaction problem at all.
**Change made:** none
