# Task: Feasibility — Z3 wrapper in Evident (FFI to libz3) (wave 5a)

## What this is

**A diagnostic / design wave.** No code changes to `kernel/`,
`compiler/`, or `stdlib/`. Deliverable = a single feasibility report
in `docs/plans/wave-5a-z3-in-evident.md`. Subsequent
implementation waves can be planned from your report's recommendation.

## Why

The long-term north star is "no Rust at all" — the kernel itself
(today ~880 LOC of Rust) replaced by Evident programs that call
libz3 (kept), libffi (kept or replaced), and the system assembler.

This wave asks: **how feasible is moving today's Z3 wrapper code
(roughly `kernel/src/z3.rs` / wherever Z3 lifecycle lives) into
Evident, using LibCall to libz3?**

Evident already calls FFI via `LibCall("libc", ..., ⟨ArgStr(...)⟩)`.
Today this is used for puts / file I/O. Calling `Z3_mk_solver` or
`Z3_solver_check` from Evident should be the same machinery — just
loaded from `libz3.dylib` instead of `libc`.

## Required reading

1. `CLAUDE.md` — esp. "Kernel runtime spec", FFI floor, and
   memory entry [[reference-z3-cross-parse-interning]].
2. `kernel/src/` — every file under here that touches libz3.
   Enumerate the surface: which Z3 C functions, with what
   signatures and lifetimes.
3. `stdlib/kernel.ev` — current `BuildPrintln`, `BuildLibCall`,
   etc. shapes for FFI sugar.
4. `docs/design/smtlib-as-compile-target.md` (if exists; otherwise
   memory [[project-smtlib-compile-target]]).
5. Memory entries: [[project-constraint-model-compilation]],
   [[feedback-aot-over-runtime-disk-cache]],
   [[project-smtlib-prototype-result]].

## Scope (what your report must answer)

### Section 1: Z3 API surface today

List every libz3 function the kernel calls. For each:
- Function name + signature (return type + each parameter type).
- Z3 reference-counting behavior (which functions take/release refs).
- Caller in the kernel (file + line range).

Use `grep -rE 'Z3_[A-Z][a-z]' kernel/src/` to enumerate.

### Section 2: LibCall feasibility per signature

Group the functions by argument shape:
- **Easy**: scalar in, scalar out (Int/Bool/String). Evident's
  current LibCall handles these directly.
- **Pointer**: takes / returns a pointer (Z3_ast, Z3_context,
  Z3_solver). Evident must marshal these as opaque Int handles.
  Identify what stdlib sugar (`BuildZ3MkContext`, etc.) would need
  to look like.
- **Struct / array**: passes-by-value structs (`Z3_symbol`,
  `Z3_string`, etc.) or arrays. These are the hardest. Identify
  count.
- **Callback**: takes a function pointer (error handler etc.).
  Identify count and which are essential.

### Section 3: Reference counting

Z3 uses manual reference counting (`Z3_inc_ref` / `Z3_dec_ref`).
Today the kernel handles this in Rust drop semantics. In Evident
there's no destructor.

Propose a model:
- Explicit `BuildZ3DecRef` calls at the end of each scope?
- A "ref-counted region" claim with lifetime built into the FSM?
- A leak-everything-until-process-exit model (acceptable for
  short-lived compiler runs)?

### Section 4: Sample claim

Write — in your report — a sample `BuildZ3MkSolver` claim showing
what the FFI sugar would look like. NOT in `stdlib/`; just in the
report doc.

### Section 5: Verdict + roadmap

A clear `feasibility: HIGH|MEDIUM|LOW|BLOCKED` verdict with a
1-paragraph justification. If HIGH or MEDIUM, sketch a 3-step
implementation roadmap (sugar-first wave, then call-sites, then
remove Rust).

If LOW or BLOCKED, name the specific blocker and what new
language / runtime capability would resolve it.

## Forbidden

- Editing `kernel/`, `compiler/`, `stdlib/`, `bootstrap/`, Python.
- Implementing any of the proposed claims (write them as
  illustration only).
- Estimating speedups — this is a feasibility report, not a
  perf hypothesis.

## Reporting back

- Branch (`agent-52-feasibility-z3-in-evident`).
- Report doc at `docs/plans/wave-5a-z3-in-evident.md` containing
  all 5 sections.
- Cite docs.

Be terse. Less than 1500 words for the whole report.
