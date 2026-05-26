# runtime/src/functionize/llm.rs — Z3-replaceability
**What it does:** Functionizer strategy that samples I/O pairs from Z3, sends them to an LLM (Anthropic API) as a prompt asking for a Rust `fn compute(...)`, compiles the returned code with `rustc` into a cdylib, validates it against held-out Z3 pairs, and caches the native function. An alternative to Cranelift that synthesizes code rather than transpiling Z3 AST.

**Criticality:** peripheral

**Verdict:** circular

**Confidence:** high

**How (if replaceable):** Same Tier-0 reasoning. This strategy IS a compilation pipeline (Z3 AST → sampled I/O → LLM-generated Rust → native cdylib). It uses Z3 as the oracle for sampling and validation, but the overall goal is to produce a native function — that's codegen, not a CSP. A Z3 solve cannot replace the act of compiling Rust source to a shared library. Also opt-in (not the default strategy).

**Change made:** none
