# runtime/src/core/value.rs — Z3-replaceability

**What it does:** Defines `Value` (the runtime value type returned in query bindings — Int/Real/Bool/Str/SeqInt/SeqBool/SeqStr/Composite/SeqComposite/SeqEnum/SetInt/SetBool/SetStr/Enum variants) and `EvalResult` (satisfied bool + bindings map + optional unsat core indices). These are the types extracted from Z3 models and handed back to callers.

**Criticality:** critical

**Verdict:** not-a-CSP

**Confidence:** high

**How (if replaceable):** Not applicable. This file is a pure data-definition module — a Rust enum and struct representing extracted Z3 model values. It IS the output of the Z3 solve; you cannot replace the type definition that holds Z3's answers with Z3 itself. Every layer of the runtime (translate/eval/decode.rs, effect_dispatch.rs, effect_loop/, functionize/) reads and writes `Value`. Tier 0 kernel.

**Change made:** none
