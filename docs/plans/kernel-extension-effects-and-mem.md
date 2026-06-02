# Kernel-extension proposal: namespaced `*_effects` channels + `__mem__` library

**Status:** PROPOSAL — awaiting user approval per the `kernel/`
freeze in CLAUDE.md.

**Why this is being proposed:** The exploration of the
`tiny-runtime` branch (see `docs/notes/python-branch-techniques.md`)
identified two runtime features that the shipped FTI design assumes
but the current Rust `kernel/` does not provide. Without one or the
other, `prelude/stack.ev` and `prelude/queue.ev` from tiny-runtime
cannot be transcribed into `compiler/`. The trade-off below is
between editing the frozen kernel or expressing the same semantics
through `++`-composed guarded writes (uglier, more brittle).

The proposal is gated; the coordinator will not implement this
without the user explicitly approving Option A vs. Option B vs.
Option C below.

## What tiny-runtime needs that we don't have

### 1. Multiple namespaced `*_effects` channels

Tiny-runtime allows a program to have *several* effects channels,
discovered by convention: `effects` (the host FSM's channel) plus
any top-level field whose name ends in `_effects` (`s__effects`,
`q__effects`, …). The runtime dispatches each channel independently
each tick. This is how FTI bodies can produce side effects without
fighting the host's `effects = ⟨…⟩` constraint.

Current kernel: one `effects` channel, single-writer rule, `++`
composition for multi-writer. The single-writer rule has been
enforced because it dodges the Z3 over-constraint problem that
`effects = ⟨a⟩ ∧ effects = ⟨b⟩` causes.

Reference: `legacy-python/docs/fti-composition.md` §"FTIs declare
their own effects channels."

### 2. The `__mem__` synthetic library

Tiny-runtime intercepts `LibCall("__mem__", "mem_alloc", …)` etc. in
its FFI layer and serves it from a Python-side ctypes backing
(`malloc`, pointer arithmetic, `*(long*)p` reads). It is the
escape-hatch for "dereference a pointer," which the current libffi
sig grammar (`i/l/d/s/v`) can't express directly.

Current kernel: pure libffi dispatch — every `LibCall` resolves to a
real `dlsym`'d symbol. No `__mem__` interception.

Reference: `legacy-python/docs/runtime-architecture.md` §"__mem__
escape hatch."

## Options

### Option A — implement both (kernel additions)

Add to `kernel/`:

- Manifest header recognises `*_effects` fields. The kernel iterates
  over each per tick, dispatches independently.
- A `__mem__` library shim that intercepts `LibCall("__mem__", …)`
  before libffi resolution, serves the four primitives
  (`mem_alloc`/`mem_load_long`/`mem_store_long`/`mem_free`) from
  Rust-side `Vec<u8>` or raw `libc::malloc`.

**Cost:** ~80–150 LOC of new Rust in `kernel/`. The kernel grows from
~880 to ~1,000–1,030 LOC.

**Benefit:** `prelude/stack.ev` and `prelude/queue.ev` from
tiny-runtime become directly transcribable. FTI design as documented
in `legacy-python/docs/fti-composition.md` works as-is.

**Cost on the freeze:** Edits Rust code that is currently frozen.
Requires explicit user approval.

### Option B — express FTI logic in the single-channel model

Re-write the FTI effect emissions as guarded writes into the single
`effects` channel, composed with `++`:

```evident
effects = host_effects ++ stack_effects ++ queue_effects
host_effects = …
stack_effects = match push_detected
    true ⇒ ⟨LibCall("libc", "memcpy_or_whatever", …)⟩
    false ⇒ ⟨⟩
```

For `__mem__`, replace it with direct libffi calls to `libc`'s
`malloc`/`memcpy`/`free` — same semantics, more verbose, but no
kernel addition.

**Cost:** Per-FTI verbosity; the legal-transition disjunction has to
be expressed differently because all writes share one channel; some
analysis bookkeeping to avoid silent overlap.

**Benefit:** Zero kernel changes. Freeze holds.

**Risk:** The single-writer rule constrains FTI authoring in ways
tiny-runtime didn't have to design around. Some FTI shapes may not
be expressible at all.

### Option C — skip Stack/Queue FTIs, jump straight to the Z3-FTI

The Z3-via-libcall FTI (`legacy-python/docs/fti-z3.md`) is a more
powerful mechanism that makes the compiler a Formula-builder
(consistent with the architectural direction the user already
articulated). It also needs three runtime additions:

- `ArgRef` + tick-local scratchpad (let one `LibCall` result feed
  another `LibCall` within the same tick).
- `@push`/`@pop` post-order RPN evaluator.
- `define-fun-rec` for the materializer recursion.

**Cost:** Larger kernel addition (~200–300 LOC).

**Benefit:** This is the architectural direction the user has already
committed to. The Stack/Queue FTIs become less central — they're a
v1 design from tiny-runtime that the Z3-FTI supersedes.

**Risk:** Larger spec to land in one go; tiny-runtime never built
this so we have no reference implementation to crib from.

## Coordinator recommendation

**C, with B as the fallback for any case where a non-Z3 FTI is
genuinely needed before C lands.** The user has stated the
compiler-as-Formula-builder direction. The Stack/Queue FTIs from
tiny-runtime were designed when that direction wasn't yet committed;
they're a v1 stopgap that the Z3-FTI eliminates. Spending kernel
budget on Option A locks us into a path the user has already
moved past.

If the user wants to land Stack/Queue first as a stepping stone:
Option B (single-channel + `++` composition + `libc::malloc` direct
calls) is preferred over Option A — no kernel changes, less freeze
disruption.

## Decision needed

User to choose one of:

1. **C only** — start the Z3-FTI work now. I'll write a follow-up
   proposal specifying the `ArgRef` + scratchpad + `define-fun-rec`
   additions in kernel-extension detail, for review.
2. **C + B as bridge** — pursue Z3-FTI, and when an intermediate
   compiler pass needs a stack/queue before Z3-FTI lands, use
   single-channel + `libc` directly.
3. **A** — implement multi-channel + `__mem__` first. (Defers the
   architectural pivot.)
4. **Hold** — pause this decision until more conformance corpus is
   built and we know which FTI shapes the compiler actually needs.

The coordinator will not implement any of the above without the
user picking one.

## Relevant files

- `legacy-python/docs/runtime-architecture.md`
- `legacy-python/docs/fti-composition.md`
- `legacy-python/docs/fti-z3.md`
- `legacy-python/docs/fti-z3-m6-extensions.md`
- `docs/notes/python-branch-techniques.md`
- `CLAUDE.md` — freeze rules
