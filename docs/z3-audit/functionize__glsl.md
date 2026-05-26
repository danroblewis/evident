# runtime/src/functionize/glsl.rs — Z3-replaceability
**What it does:** Transpiles pure scalar Int/Bool `Z3Program`s to GLSL fragment shaders; runs a headless OpenGL (CGL, macOS only) 1×N draw pass and reads back results via `glReadPixels` on a `RGBA32I` texture. An alternative JIT strategy using the GPU rather than Cranelift.

**Criticality:** peripheral

**Verdict:** circular

**Confidence:** high

**How (if replaceable):** Same Tier-0 reasoning as Cranelift: this is a codegen/compilation strategy. It transpiles Z3 AST to GLSL source and orchestrates GPU execution. A Z3 solve cannot replace a GPU-execution pipeline. Also macOS-only and not the default strategy (`default()` returns Cranelift).

**Change made:** none
