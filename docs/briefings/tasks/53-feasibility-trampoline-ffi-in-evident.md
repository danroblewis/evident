# Task: Feasibility — Trampoline + FFI dispatch in Evident (wave 5b)

## What this is

**A diagnostic / design wave.** No code changes. Deliverable = a
feasibility report in `docs/plans/wave-5b-trampoline-ffi-in-evident.md`.

## Why

Today the kernel's FFI dispatch uses libffi (a C library) via a
Rust shim. The trampoline loop drives the FSM: solve →
dispatch_effect → solve → ...

The long-term north star is "no Rust." libffi can stay (it's C),
but the Rust shim around it should move into Evident. Going one
step further: we could replace libffi entirely with per-architecture
call-site builders described in Evident as constraint models over
the calling convention.

This wave asks: **what would each path look like, and which is
more feasible?**

## Required reading

1. `CLAUDE.md` — Effect enum floor (LibCall, etc.).
2. `kernel/src/libcall.rs` and `kernel/src/effects.rs` (whatever
   the effect dispatcher is) — read the libffi usage.
3. `stdlib/kernel.ev` — current `BuildPrintln`, `BuildLibCall`
   sugar.
4. Memory: [[project-fti-honesty-audit-result]] (FTI is the
   Foreign Type Interface — composable libc-backed types) and
   [[project-functionize-walk-result]].

## Scope

### Section 1: Today's trampoline + FFI surface

Describe in 1-2 paragraphs:
- The kernel's main loop (tick → solve → dispatch_effect → tick).
- Where libffi is used (call sites, what it dispatches).
- The Rust-side bookkeeping the loop maintains (state pinning,
  result marshaling).

### Section 2: Path A — keep libffi, move loop to Evident

Sketch what the Evident-side main loop would look like. It would
need to:
- Call `Z3_solver_check` (covered by wave 5a — assume done).
- Read the model into state pins (string formatting; doable).
- Dispatch `LibCall` effects via something like
  `BuildLibCallWith(libffi_ctx, ...)` — a sugar over libffi's
  `ffi_call`.

Identify the FFI signatures of libffi calls. They're mostly
pointer-passing; assume wave 5a's marshaling story handles them.

### Section 3: Path B — replace libffi with Evident codegen models

Sketch what an Evident-described calling-convention codegen
would look like. Per architecture (x86-64 sysv, ARM64 aarch64,
RISC-V):
- A `claim AmdCall(...)` style constraint over arg slots → register
  assignments + stack writes.
- An emit pass that produces a byte sequence (the call stub).
- Loaded into executable memory via `mprotect` + `LibCall`.

Note the cross-platform multiplier: ONE Evident codegen model per
architecture × however many tools use it.

### Section 4: Comparison

Table:

| Aspect | A (keep libffi) | B (Evident codegen) |
| ------ | --------------- | ------------------- |
| Lines of new code (rough) | ? | ? |
| Cross-platform out of box | yes (libffi handles it) | per-arch model |
| Dependency story | one C lib | none beyond libc + as |
| Bootstrap risk | low | medium |
| Compile-time speed | fast (libffi cached) | depends on codegen |

### Section 5: Verdict + roadmap

`feasibility: HIGH|MEDIUM|LOW|BLOCKED` for each path. Recommend
which to ship first (sequence matters — A first then B is the
natural progression).

If LOW or BLOCKED, name the specific blocker.

## Forbidden

- Editing `kernel/`, `compiler/`, `stdlib/`, `bootstrap/`, Python.
- Implementing the proposed designs.
- Including the wave-5c (functionizer) work — that's a separate
  wave.

## Reporting back

- Branch (`agent-53-feasibility-trampoline-ffi-in-evident`).
- Report at `docs/plans/wave-5b-trampoline-ffi-in-evident.md`.
- Cite docs.

Less than 1500 words.
