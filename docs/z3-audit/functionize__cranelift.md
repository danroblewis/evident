# runtime/src/functionize/cranelift.rs — Z3-replaceability
**What it does:** JIT-compiles a `Z3Program` to native machine code via Cranelift, generating an `extern "C"` function that evaluates the constraint's defining expressions directly (bypassing Z3 on the hot per-tick path). This is the default and primary functionizer strategy.

**Criticality:** critical

**Verdict:** circular

**Confidence:** high

**How (if replaceable):** Tier-0 bootstrap compiler per the smtlib-as-compile-target design. A Z3 solve cannot replace the code that compiles Z3 ASTs to native code — that is the bootstrap compiler itself. Replacing Cranelift with a Z3 solve would require the Z3 solve to produce native machine code, which is precisely what this is. Per the audit brief: "functionize/ are the JIT/codegen STRATEGIES — they are the bootstrap COMPILER that turns a Z3 AST into native code. A solve can't replace the thing that compiles solves."

**Change made:** none
