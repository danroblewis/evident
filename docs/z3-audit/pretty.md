# runtime/src/pretty.rs — Z3-replaceability
**What it does:** Stable diagnostic-render entry points (`pretty::expr`, `pretty::body_item`). Delegates entirely to the `pretty_walk` stack-FSM in `stdlib/passes/pretty.ev` via `EvidentPretty` (cached, thread-local). Falls back to `{:?}` on re-entrancy or stdlib unavailability.
**Criticality:** peripheral
**Verdict:** circular
**Confidence:** high
**How (if replaceable):** Already self-hosted — this file IS the thin Rust shim over the self-hosted Evident pretty-printer (the `pretty_walk` FSM). The rendering IS a Z3 solve (each render calls `run()` over the AST). The file itself is ~55 lines of plumbing (thread-local cache, re-entrancy guard, two pub fns). Nothing further to replace.
**Change made:** none
