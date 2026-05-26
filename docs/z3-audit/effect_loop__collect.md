# runtime/src/effect_loop/collect.rs — Z3-replaceability
**What it does:** Collects dispatchable `Effect` values from a solved binding map: in Mode 1 decodes the primary `Seq(Effect)` slot directly; in Mode 2 walks all bindings to find Effect-valued vars, builds synthetic node names, infers ordering edges from SeqLit body constraints, then delegates to `evident_toposort` (which is already a Z3 solve) for ordering.
**Criticality:** critical
**Verdict:** not-a-CSP
**Confidence:** high
**How (if replaceable):** This is a post-solve traversal over Rust `Value` maps — the problem isn't "find a satisfying assignment"; it's "decode what Z3 already returned and order it." The ordering sub-problem IS already done via Z3 (delegated to `evident_toposort`); the surrounding collect/decode/denormalize logic is pure Rust data manipulation with no constraint content. A Z3 solve over the full collect logic would require encoding the whole `Value` hashmap as Z3 terms, which is circular (Z3 just produced those values).
**Change made:** none
