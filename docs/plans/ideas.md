# Ideas — deferred until after bootstrap deletion

Things we want to do *after* the deletion path is complete. Not on
the critical path; not blocking; not to be picked up by sessions
until `scripts/check-deletable.sh` exits 0.

## BNF parser-generator in Evident

**Source:** user, mid-session ~task #18.

**Idea:** describe Evident's grammar in BNF (one file, e.g.
`compiler/evident.bnf`), and build a generic BNF parser-generator
in Evident that:

1. Reads a BNF file.
2. Emits a working lexer + parser as Evident code (or runs them
   directly as interpreters over the grammar).
3. Works for any BNF grammar, not just Evident's.

**User rationale:**

> *"It would be really cool if we could make a BNF parser in
> Evident, if we could describe our Evident grammar in BNF then
> use the BNF file to generate a parser and lexer and work from
> that. A generic BNF in Evident that we could use for any BNF
> grammar. I think future agents trying to modify the grammar
> rules and syntax might have an easier time working on BNF than
> on Evident code describing the grammar."*

**Why defer:**

- Not on the bootstrap-deletion critical path. The current
  hand-written `compiler/lexer.ev` and `compiler/parser.ev` are
  good enough to produce `compiler.smt2` once they compose; a
  BNF-driven equivalent is a refactor of working code.
- A parser-generator is substantial — easily its own
  multi-session arc. Spawning it now risks contention with the
  critical-path work.
- After bootstrap is deleted, the grammar surface is whatever the
  self-hosted compiler accepts. The BNF + generator can replace
  the hand-written passes cleanly with no
  bootstrap-equivalence concern.

**When to pick this up:** after Phase 5 of
`docs/plans/DELETION-CHECKLIST.md` (bootstrap severed from all
test paths). At that point, this becomes a clean follow-up.

**Likely shape when implemented:**
- `compiler/evident.bnf` — Evident's grammar in BNF.
- `compiler/bnf_lexer.ev` — generic BNF tokeniser.
- `compiler/bnf_parser.ev` — generic BNF parser, producing a
  grammar AST.
- `compiler/bnf_generate.ev` — emits a lexer + parser specialized
  to a given grammar AST. Either as Evident source (compile-time
  generation) or as a runtime interpreter (generic but slower).
- New tests that demonstrate the generator on at least two
  grammars (Evident's own + one other, e.g. JSON or arithmetic).

## Replace Cons-lists with Seqs (constraint-model fit)

**Status: carry capability LANDED; work-stack sweep BLOCKED on perf —
see `docs/plans/blocked-cons-to-seq-perf.md`.**
The functionizer side landed in task #19 — `recompose_record_seqs` is
in `kernel/src/functionize/` (see step 1 below and
`functionizer-integration.md` §6), so a bounded `Seq(Record)` now
*functionizes* at parity with a cons-list. The **Seq-as-state carry
capability** landed in task #21 (second run): bootstrap
`discover_state_fields`/`render_prev_state_decl` emit `Seq(Elem)` state
fields, `translate_seq_value` lowers the carry-assignment (`seq =
(cond ? ⟨…⟩ : _seq ++ ⟨x⟩)`, `seq_init` drop-last, whole-array equality
with a *symbolic* length), and the kernel reads/pins it (`read_seq_var`
+ `emit_state_pin`). Proven by `tests/kernel/test_seq_carry.ev` (green
in all 3 modes). This closed the *foundation* blocker the first run of
task #21 documented (`blocked-cons-to-seq-sweep.md`).

But the **sweep of the translator work-stacks** (steps 2–4) is still
blocked — now on *performance*, not capability. Carrying a push-heavy
work-stack as `Seq(WorkItem)` is correct (byte-identical output) but
~250× slower on Z3 than the cons-list: the binop expansion becomes a
symbolic-index array store-chain + whole-array extensional equality,
which Z3's array theory handles far worse than an algebraic-datatype
constructor. The FTIs (`IntStack`/`IntQueue`) hit it harder (Seq
equality inside the legal-transition disjunction). Per acceptance #6,
the work-stack `.ev` conversions are not shipped; the cons-lists stay.
Append-light / streaming Seq state (the capability fixture) is cheap
and *does* ship. The fix is a cons-cell-backed `Seq` lowering with
front-pop (Seq surface, `__SeqOf_T` datatype internal — as fast as cons
because it *is* cons), or functionizer support for symbolic-index
arrays. Details + reproduction in the perf blocker doc.

**Source:** user, mid-session ~task #18.

**Idea:** the current invariants point sessions at cons-list state
(`enum WorkList = WLNil | WLCons(WorkItem, WorkList)`) for bounded
data, because cons-lists functionize cleanly today via the
macro-finder while Z3 Seqs are opaque to it. But cons-lists are
imperative-shaped — they carry a "first/rest" verb structure —
and that doesn't naturally appear in constraint-model thinking.
Seqs are the more constraint-native shape.

**User rationale:**

> *"I don't like using the Cons things because we never seen Cons
> in constraint system models. Seq made more sense, and I would
> like to see if we can replace Cons with Seq, even if it has to
> be some rewrite rules."*

**Why the sweep is still deferred:**

- ~~The functionizer's `recompose_record_seqs` path was deferred in
  task #18.~~ **Done in task #19** — no longer a blocker.
- The shift is a sweeping rewrite: every `WorkList` /
  `WLCons`-style pattern in `compiler/translate_*.ev`,
  `stdlib/fti/*.ev`, and the AST walkers would change.
- Doing it before bootstrap deletion adds risk to an
  already-large refactor.

**Likely path when picked up:**

1. ~~Land the `recompose_record_seqs` functionizer extension.~~
   **Done (task #19):** `kernel/src/functionize/{mod,eval,jit}.rs`,
   exercised by `tests/kernel/test_functionizer_seqs.ev`.
2. ~~Add a Seq carry capability + a Seq-based work-stack pattern.~~
   **Capability done (task #21):** Seq carries as state
   (`tests/kernel/test_seq_carry.ev`). But the array+len encoding the
   capability uses is ~250× slower on Z3 for the push-heavy work-stack
   shape (symbolic-index stores + whole-array equality). So before the
   sweep, swap the carry encoding for a **cons-cell-backed `Seq` with
   front-pop** (`__SeqOf_T` datatype internal, `Seq` surface) — as fast
   as cons because it *is* cons — or add functionizer support for
   symbolic-index arrays. See `blocked-cons-to-seq-perf.md`.
3. Sweep the codebase replacing cons-list state with Seqs,
   one pass at a time, verifying each retains its
   `tests/conformance/features/` equivalence **and** stays within the
   2× per-tick perf gate.
4. Drop the cons-list pattern from the invariants doc.

**When to pick this up:** after Phase 5 of
`docs/plans/DELETION-CHECKLIST.md` (bootstrap severed). Or sooner
if the cons-list pattern starts creating noticeable maintenance
friction.

## FTI honesty audit: rewrite Stack/Queue to actually use external memory

**Source:** user, mid-session ~task #19.

**The current FTI is anti-pattern stacked three ways:**

1. The "Stack contents" live in Z3 via an `IntStack` cons-list
   (`enum IntStack = SEmpty | SNode(Int, IntStack)`). The entire
   structure is in the Z3 model on every tick, growing it.
2. The `libc::malloc(1024)` the FTI emits is a **write-only
   shadow**. Each push emits `memset(base+offset, value, 1)`,
   but nothing in the FTI ever reads those bytes back. The libc
   memory is decorative; it does no work for the program.
3. The FTI never emits `free()`. Today this is masked because
   processes exit and the OS reclaims memory, but a long-running
   program creating and destroying Stack/Queue instances
   accumulates them.

**User rationale:**

> *"Does it populate the entire queue/stack of Cons cells in Z3
> solver memory? Because that would be an anti-pattern. Or does
> it somehow use the FTI interface and LibCall's to leverage
> external memory and keep the Z3 solver lean? I also notice we
> call malloc but I never see us calling free, so do we have a
> built-in memory leak here?"*

Both observations are correct, plus the deeper problem (the libc
memory not being the source of truth).

**Honest FTI design** (what we'd want):

- Z3 side carries only metadata: `base ∈ Int` (pointer), `depth ∈ Int`,
  `top ∈ Int` (top-of-stack value pulled in via a per-tick read so
  the FSM can dispatch).
- Data lives in libc memory. Push = `memset(base+depth*8, value, 8)`.
  Pop = reduce depth; optionally read the new top via libcall on
  next tick.
- Teardown phase emits `free(base)` when the FTI's host FSM enters
  its terminal state.
- This requires a per-tick `mem_load` primitive (which legacy-python
  called `__mem__::mem_load_long` and we declined to add to the
  kernel). To avoid kernel additions, an alternative is a
  one-tick-latency libcall to a generic `int (*)(long)` reader
  function we'd plant in libc — feasible with the existing libffi
  surface.

**Why defer:**

- Touches multiple FTI implementations, the host-side test fixtures,
  and probably the libffi sig grammar to support pointer-load
  arguments. Real work.
- The current FTI passes tests because the cons-list IS doing the
  work; the libc shadow is harmless ceremony. So nothing is broken
  user-visibly today.
- Better done together with the cons→Seq sweep (the two share
  motivation: get state out of Z3).

**When to pick this up:** after `recompose_record_seqs` lands
(task #19) and the cons→Seq sweep starts. The honest-FTI redesign
naturally piggybacks: as we move cons-lists out of Z3, the FTI's
"data in Z3" anti-pattern gets the same treatment.

**Likely shape:**
1. Add a generic `mem_load_long(base+offset) → long` via libffi
   wrapper (no kernel change; pure FTI Build* sugar).
2. Rewrite `stdlib/fti/stack.ev` and `stdlib/fti/queue.ev` to
   keep `contents` out of the Z3 side; carry only `(base, depth, top)`.
3. Add `BuildStackFree(eff)` to emit `LibCall("libc","free",base)`.
4. Add a teardown-on-exit pattern (probably an `is_halting ∈ Bool`
   the host sets to true on terminal tick, triggering the free).
5. Update `tests/kernel/test_fti_stack.ev` and `test_fti_queue.ev`
   to exercise free.
6. Verify no regression.

## FTI wrappers around C-library data structures

**Source:** user, mid-session ~task #23.

For FTIs covering structures more complex than Stack/Queue (hash
tables, balanced BSTs, priority queues), wrap mature C libraries
rather than reimplementing: glib's GHashTable/GTree/GQueue/GArray,
libavl, sys/queue.h (BSD-style intrusive lists; in libc, no extra
dep), libdatrie/libcritbit for tries. The FTI body carries just a
pointer + lightweight metadata; each op is a `LibCall` into the
library API. Z3 model stays small; the C library does the work
it's good at.

User rationale:

> *"We can try to look for C libraries that implement
> datastructures for us, and we can build our FTI models to wrap
> the state machines of the library."*

**When to pick this up:** when a real perf problem in
`compiler/*.ev` calls for it (likely the symbol table). Until
then, the honest-FTI pattern from task #23 is the default for new
FTIs.

## (Add more ideas here as they surface)
