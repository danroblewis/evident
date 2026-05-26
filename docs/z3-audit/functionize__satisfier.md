# runtime/src/functionize/satisfier.rs — Z3-replaceability
**What it does:** Functionizer strategy for `Z3Program`s containing `SampleRange`/`SampleEnum`/`SampleSet` steps. Draws the sampled variables via a seeded SplitMix64 PRNG (no Z3 call), then delegates the deterministic remainder to Cranelift. Bridges the gap between purely sampled programs and JIT-compiled ones.

**Criticality:** peripheral

**Verdict:** circular

**Confidence:** high

**How (if replaceable):** This is a codegen/compilation strategy: it compiles a `Z3Program` into a `CompiledFunction` (native callable). The PRNG sampling is the "satisfying assignment" for the sampled vars; the residual is handed to Cranelift JIT. The compilation pipeline character makes it Tier-0 / circular — a Z3 solve cannot produce the `CompiledFunction` artifact this generates. The PRNG logic itself (`splitmix64`, `fnv1a64`, `seed_state`) is pure math with no CSP structure.

**Change made:** none
