# Post-cutover roadmap — what to do AFTER `rm -rf bootstrap/`

The bootstrap-deletion cutover is gated on one compiler fix
(`compiler/translate_ctor.ev` — see STATE.md "THE single ctor-arg
blocker"). This file is the plan for the *next* phase: how the
kernel itself shrinks toward zero.

Today the kernel is ~880 LOC of Rust: trampoline + libffi + Z3
wrapper. Each piece has a feasibility study; this roadmap names
them, sequences them, and quotes their verdicts.

## The four phases (in dependency order)

### Phase 1 — Z3 wrapper in Evident
**Plan:** [`wave-5a-z3-in-evident.md`](wave-5a-z3-in-evident.md)
**Verdict:** `MEDIUM` (split — solve half HIGH, model-readback half
BLOCKED on two named capabilities).

Replace the Rust calls to the ~70 Z3 C functions in
`kernel/src/{tick,functionize/*}.rs` with `LibCall("libz3", "...")`
from Evident. Solve loop (parse → assert → check → read sat int) is
HIGH; decoding the model AST into effect values needs new primitives.

### Phase 2 — Trampoline + libffi in Evident
**Plan:** [`wave-5b-trampoline-ffi-in-evident.md`](wave-5b-trampoline-ffi-in-evident.md)
**Verdict:** Path A `HIGH` (libffi stays a pure C dep, just call its
entry points from Evident). Path B `MEDIUM` (replace libffi entirely
with mmap+mprotect+codegen — bigger lift, Apple Silicon W^X / MAP_JIT
story is the real cost).

Ship A first: it depends only on phase 1's `Z3_solver_check`
machinery plus a trivial `dlsym_addr` addition. B reuses A's
handle-passing substrate and is the prerequisite for phase 3 option Z.

### Phase 3 — Functionizer in Evident
**Plan:** [`wave-5c-functionizer-in-evident.md`](wave-5c-functionizer-in-evident.md)
**Verdict:** Recognizer half `HIGH` (tree-walk over Z3 ASTs — same
shape as validate_walk and desugar). Codegen half splits:
| Codegen option | feasibility | one-line note |
| -------------- | ----------- | -------------- |
| X — emit asm, shell out to `as`, dlopen | `HIGH` | fastest to "no Rust"; needs phase-2 `dlopen` |
| Y — link libLLVM via FFI | `LOW` | heavy permanent dep, only edge is quality |
| Z — self-hosted ISA models | `MEDIUM` | "Evident all the way down" endgame; needs phase-2 exec pages |

Prototype X first. Z is the final form.

### Phase 4 — AOT binary cache
**Plan:** [`wave-5d-aot-binary-cache.md`](wave-5d-aot-binary-cache.md)
**Verdict:** `MEDIUM` (HIGH for the side-car format on all-scalar
programs; residual-step dependency on a live Z3 context is what keeps
it from HIGH across the board).

AOT functionization with a disk cache **is** compilation to a binary
— same operation as today's JIT, moved from per-run to build time and
persisted. Once phase 3 lands, this is what materializes the
generated code into something the kernel just loads.

## Cross-wave dependency notes

- Phase 1's FFI sugar must cover the Z3 **tactic/goal** API
  (`Z3_mk_goal`, `Z3_mk_tactic`, `Z3_tactic_apply`) in addition to
  context/solver lifecycle. The functionizer recognizer needs it.
- Phase 2 must expose `dlopen`/`dlsym` (for phase 3 option X) and
  `mmap`+`mprotect` (for option Z).
- Phase 3 has a chicken-and-egg property: the recognizer needs to
  *functionize itself*. Solve by keeping the Rust stage-0
  functionizer until phase 4 closes the loop.

## End state

Kernel/ has been shaved down to the trampoline shim and a couple of
syscalls; everything that used to be `Z3_*` or `libffi_*` is now an
Evident program calling `LibCall("libz3", "...")` / `LibCall("libc",
"dlsym")`. The "kernel + compiler.smt2" pair is no longer just the
compiler — it is the whole runtime.

## Where to pick up

The ordering above is a *suggestion*, not a constraint. Phase 1 is
the obvious next step because everything else depends on it, but a
session that just wants to ship the cutover should focus on
`compiler/translate_ctor.ev` first and not touch any of phases 1-4.
The feasibility plans are durable reference; the actual ordering of
phase work will follow what unblocks the most LOC reduction per
session.
