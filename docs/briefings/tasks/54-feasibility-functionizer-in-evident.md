# Task: Feasibility — Functionizer in Evident (wave 5c)

## What this is

**A diagnostic / design wave.** No code changes. Deliverable = a
feasibility report in `docs/plans/wave-5c-functionizer-in-evident.md`.

## Why

The functionizer is the keystone of "no Rust kernel": it's the
piece that compiles Z3 ASTs into native code at load time. The
recognizer half (Z3 tactic chain → Step shapes) is pure data
manipulation and should be expressible in Evident as constraint
walks over Z3 ASTs (via the wave-5a Z3 FFI sugar).

The codegen half is where Cranelift lives today. Replacing
Cranelift is the hard part. This wave surveys the choices.

## Required reading

1. `kernel/src/functionize/mod.rs` and `kernel/src/functionize/eval.rs`
   and `kernel/src/functionize/jit.rs` — the whole module.
2. `legacy-rust/functionizer/` — the extracted reference
   implementation memory mentions
   ([[project-functionizer-macro-finder-extracted]]).
3. `docs/plans/grammar-wave4e-perf-diagnostic.md` and
   `docs/plans/wave-4r-pertick-hot-shapes.md` — what the
   functionizer extracts and refuses on compiler.smt2.
4. Memory entries: [[project-constraint-model-compilation]],
   [[project-smtlib-compile-target]].
5. `compiler/translate_ctor.ev` — an example of complex Evident
   code that's already constraint-shaped. Helpful to gauge what
   the recognizer would look like.

## Scope

### Section 1: Recognizer half — feasibility in Evident

The recognizer:
- Takes a Z3_ast body assertion.
- Pattern-matches it to a Step shape (scalar, guarded, seq).
- Emits a Step structure for later codegen.

This is essentially a tree walk with pattern matching. Evident
already does similar walks (translate_ctor's renderers,
parse_body's membership walks). Assess: with wave-5a's Z3 FFI
sugar in place (assume yes), what does
`MatchFunctionizableStep(ast ↦ a, out ↦ step)` look like?

Sketch the claim (2-page max).

### Section 2: Codegen half — three options

Compare three concrete options:

**Option X: shell out to `as`**
- Evident emits assembly text.
- Calls system assembler via `LibCall("libc", "system", ...)` or
  `LibCall("libc", "execv", ...)`.
- Loads result via `dlopen` or `mmap` + executable pages.
- Pros: simplest. No new codegen code.
- Cons: per-call disk write + assembler invocation overhead.

**Option Y: libLLVM via FFI**
- Use libLLVM through Evident's LibCall mechanism.
- Full optimizer for free.
- Pros: best codegen quality.
- Cons: libLLVM is C++ (no Rust but a big C++ dep). Versioning.

**Option Z: self-hosted instruction-set models**
- Per-architecture Evident model describing the encoding (x86-64
  ModR/M bytes etc.).
- Direct byte-sequence emit.
- Pros: cleanest "Evident all the way down" story.
- Cons: big undertaking; one model per architecture.

For each: 1 paragraph each on viability + dev time estimate.

### Section 3: Hybrid path

You don't have to pick one. Sketch a sequencing where Option X
ships first (gets us to "no Rust"), Option Z lands later for
specific hot paths (gets us to "Evident all the way down"), and
Option Y stays as a fallback for complex shapes.

### Section 4: Verification story

The functionizer today verifies extracted programs against Z3
solves (tick-0 + tick-1 equivalence checks). The Evident port
should keep this — sketch how it'd work.

### Section 5: Verdict + roadmap

`feasibility: HIGH|MEDIUM|LOW|BLOCKED` for each codegen option.
Recommend which to prototype FIRST (probably Option X — fastest
to ship). 3-step roadmap.

## Forbidden

- Editing `kernel/`, `compiler/`, `stdlib/`, `bootstrap/`, Python.
- Implementing any option.
- Estimating speedups vs the current Cranelift JIT — assume
  parity is the target, not improvement.

## Reporting back

- Branch (`agent-54-feasibility-functionizer-in-evident`).
- Report at `docs/plans/wave-5c-functionizer-in-evident.md`.
- Cite docs.

Less than 2000 words.
