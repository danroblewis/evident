# runtime/src/functionize/symbolic.rs — Z3-replaceability
**What it does:** Functionizer strategy that performs genetic-programming (GP) symbolic regression: samples I/O pairs from Z3, evolves a population of expression trees (`SExpr`) using tournament selection + crossover/mutation until a zero-error tree is found, then wraps it as a `CompiledFunction`. Scalar Int/Bool only, limited to ≤4 inputs and ≤6 outputs.

**Criticality:** peripheral

**Verdict:** circular

**Confidence:** high

**How (if replaceable):** Same Tier-0 reasoning as the other functionizers: the goal is to produce a native `CompiledFunction` (a compiled expression tree callable without Z3). The GP search is a program-synthesis step — using Z3 as an oracle for samples but ultimately producing a callable artifact. The symbolic regression loop is a search problem, but the "solution" is native code, not a Z3 model. A Z3 solve cannot replace the act of discovering and hosting a callable expression tree. Also opt-in (not the default strategy).

**Change made:** none
