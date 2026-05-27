# runtime/src/portable/pretty.rs — Z3-replaceability
**What it does:** The Rust shim and trait definitions for the already-fully-cut-over pretty-printer. Defines `PrettyImpl` (the two render entry points: `expr` and `body_item`) and `EvidentPretty` (the sole implementation, which drives the `pretty_walk` stack-FSM in `stdlib/passes/pretty.ev`). The former native `RustPretty` was deleted in session pretty-evident once `pretty.ev` became byte-faithful; this file is now a thin, ~60-line wrapper with no independent logic.
**Criticality:** peripheral (load-time / diagnostic only — used for UNSAT diagnostics and `evident check` output; never on the per-tick scheduler path)
**Verdict:** replaceable-as-group(portable/pretty.rs, stdlib/passes/pretty.ev)
**Confidence:** high
**How (if replaceable):** Already fully exploited — `pretty_walk` in `pretty.ev` IS the sole renderer; this Rust file is ~60 lines of glue (trait definition, `EvidentRunner` construction, seed-wrapping, result-unwrapping). There is no Rust logic remaining that does rendering work. The one known residual (`EReal` → `<real>`, since Z3 has no real→string primitive) is documented in the pass header and is harmless because no `.ev` file uses reals.
**Change made:** none
