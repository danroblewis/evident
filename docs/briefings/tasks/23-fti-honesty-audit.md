# Task: FTI honesty audit — rewrite Stack/Queue to actually use external memory

## Why and authorisation

User authorised this as #3 in the 4-task queue. The current
`stdlib/fti/stack.ev` + `queue.ev` are anti-pattern stacked three
ways (verified in `docs/plans/ideas.md` §"FTI honesty audit"):

1. The "contents" live in Z3 via an `IntStack`/`IntQueue` cons-list.
2. The `libc::malloc(1024)` is a write-only shadow — `memset` on
   push, nothing ever reads it back.
3. No `free()` anywhere — guaranteed leak (OS-reclaimed today, but
   bad for any long-running multi-FTI program).

User framing:

> *"I never see us calling free, so do we have a built-in memory
> leak here?"*

Yes. And:

> *"Does it populate the entire queue/stack of Cons cells in Z3
> solver memory? Because that would be an anti-pattern."*

Also yes. This task fixes both.

Authorisation envelope: same as previous FTI / kernel work. You
may edit `stdlib/fti/*.ev`, test fixtures, and `kernel/src/` if a
new libffi primitive is genuinely needed. Document any kernel
edits explicitly in the report.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md` — FTI rules,
   single-channel effects, functionizability guidance.
3. `docs/plans/ideas.md` §"FTI honesty audit" — the desired design
   (metadata-only Z3 state; data in libc; free on teardown).
4. `legacy-python/docs/fti-composition.md` — the legal-transition
   disjunction pattern + the original FTI design intent.
5. `legacy-python/docs/runtime-architecture.md` — the `__mem__`
   library reference (we are NOT adopting `__mem__`; this is for
   context on what shape an honest FTI takes).
6. `stdlib/fti/stack.ev`, `stdlib/fti/queue.ev` — current state.
7. `tests/kernel/test_fti_stack.ev`, `test_fti_queue.ev` — current
   tests.
8. `kernel/src/libcall.rs` (or wherever libffi dispatch lives) —
   to see what arg signatures are currently supported.

Cite #3, #4, and #6 in your report.

## What you're producing

### New Stack FTI (`stdlib/fti/stack.ev`)

The Z3 side carries ONLY metadata:

- `base ∈ Int` — pointer returned by malloc, carried across ticks
  via `_base`.
- `depth ∈ Int` — current count of pushed values.
- `top ∈ Int` — top-of-stack value read back via a libcall (next
  tick visibility — see the two-tick handle-threading pattern in
  legacy-python/docs/runtime-architecture.md).
- `is_init ∈ Bool` — first-tick allocation guard.

The cons-list `IntStack` enum is GONE. State carry is integer
fields only.

Operations:
- Push (per tick): emit `LibCall("libc", "memcpy"|"memset", base+depth*8, value, …)`
  to write the value into libc-backed memory. Then on the next
  tick when the host reads `top`, it gets the new top via a
  read libcall.
- Pop: emit no libcall — just decrement `depth`. The popped value
  is read via the `top` libcall.
- Teardown: when the host sets `is_halting ∈ Bool = true`, emit
  `LibCall("libc", "free", base)`.

### New Queue FTI (`stdlib/fti/queue.ev`)

Same shape, FIFO. The `top` is the front of the queue. Enqueue
writes at `base + tail*8`; dequeue advances a `head` pointer.

### Test fixtures

Update `tests/kernel/test_fti_stack.ev` and `test_fti_queue.ev`:
- Push 3 / pop 2, watching `depth` evolve (same as before).
- ALSO verify the `top` value is read back correctly (the
  honesty check — the previous version never did this because
  the libc memory was decorative).
- Exercise the teardown path on the last tick: set `is_halting`
  and verify a `LibCall("libc","free",...)` appears in `effects`.

## Libffi primitives you may need

To read an int from a memory address, libffi sig `l(l)` (a function
taking a long, returning a long) should already work. You don't
need `__mem__` — `libc` has the primitives if you wrap them
correctly. Possible options:

- Just dereference via inline assembly in a tiny libc helper —
  probably overkill.
- Use existing libc functions that read memory and return: e.g.,
  there's no obvious one-shot reader. But you can chain: write a
  one-line C wrapper compiled into a small shared library that
  the FTI loads via `dlopen` and calls.
- Or pragmatically: use `libc::read`/`pread` from a pipe set up
  at FTI init time. Overkill.

The simplest path: if libffi can't currently return data from an
arbitrary address, ADD a kernel-side primitive `__mem_read_long`
that reads `*(long*)addr` and returns it. This is a one-function
kernel addition, ~20 LOC. Document it as a kernel-extension AND
defend why it's not the legacy `__mem__` library (it's the minimal
single-function escape hatch the FTI honesty requires).

## Acceptance

1. `stdlib/fti/{stack,queue}.ev` carry NO cons-list. State is
   integer metadata only.
2. `tests/kernel/test_fti_{stack,queue}.ev` pass, and verifiably
   exercise the libc-backed memory (the `top` read returns the
   correct value).
3. The FTIs emit `LibCall("libc","free",base)` on teardown.
4. `./test.sh` is fully green in all 3 modes.
5. `scripts/check-deletable.sh` unchanged (this is FTI / kernel
   work, not deletion-path).
6. Updated `docs/plans/ideas.md` §"FTI honesty audit" with a
   COMPLETE / PARTIAL marker.
7. The cons-list bloat in the Z3 model is verifiably gone — run
   `EVIDENT_FUNCTIONIZE_STATS=verbose` (from task #22) on the new
   stack fixture; the `top`/`depth` fields should appear as JIT
   or interp steps (not opaque).
8. Diff scoped to `stdlib/fti/*.ev`, test fixtures, possibly
   `kernel/src/libcall.rs` if a new primitive is needed.

## Forbidden

- Editing `bootstrap/`, `compiler/`, anything outside the named
  paths.
- Adding Python.
- Adopting the legacy `__mem__` library — keep any kernel
  addition minimal (single function, justified).
- Multi-channel `*_effects` patterns.
- Calling `.simplify()` inside the tick loop.

## Reporting back

- Branch pushed.
- Files modified.
- The `STATS=verbose` capture on the new stack fixture proving
  Z3 doesn't carry a cons-list anymore.
- The `effects` Seq showing `free(base)` on teardown tick.
- `./test.sh` final line, all 3 modes.
- Cite docs.

If you need to add a kernel primitive: justify it in one
paragraph, scope it minimally, document it in
`docs/plans/architecture-invariants.md`.
