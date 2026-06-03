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

**STATUS: COMPLETE (task #23, agent-23-fti-honesty-audit).** All three
anti-patterns fixed. `stdlib/fti/{stack,queue}.ev` now carry NO cons-list
(verified: `IntStack`/`SNode`/`SEmpty` count = 0 in the emitted SMT-LIB;
manifest state-fields carry `depth:Int` + `base:Int`, no `contents`).
Data lives in libc-`malloc`'d memory: push WRITES the full long via a new
`__mem::write_long` and the top/front is READ BACK via `__mem::read_long`
(the test drains all three pushed values straight out of C memory — they
were never in Z3). Teardown emits `LibCall("libc","free",base)` on the
host's `is_halting` tick (proven live: freeing a bogus pointer SIGABRTs;
the valid `base` frees and exits 0). One minimal kernel primitive added —
the `__mem` read_long/write_long deref pair (NOT the legacy `__mem__`
library); justified + documented in `architecture-invariants.md` §"The
`__mem` deref primitive". `./test.sh` green; both fixtures pass under all
three functionizer modes (default / JIT=0 / FUNCTIONIZE=0). Note on
acceptance #7: the fixtures still don't fast-path the *whole* tick, but
the refusal reason is no longer a cons-list — it is `(_ is IntResult)`
(the `last_results` Result-decode every memory read-back needs) plus
`str_from_int` string formatting for the `puts`; both are inherent to any
FSM that reads memory back and prints, and were equally true of the old
cons-list fixture. The `depth` metadata is now pure Int arithmetic. The
original deferral analysis follows for the record.

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

## FTIs self-owned init/teardown via nested subclaims (NO new keyword)

**Source:** user, mid-session ~task #29.

**Observation:** The current Stack/Queue FTIs (post task #23) have a
correct shape — metadata-only Z3 state, libc-backed contents, `free()`
on teardown — but the FTI itself **does not emit** its own
initialization libcalls. `BuildStackAlloc` exists; the host is
responsible for composing it and including `⟨alloc_elem⟩` in its
effects literal on `is_first_tick`. The host has to know about the
FTI's setup needs.

**The clean shape exists already.** No new keyword is required. The
parts all exist in bootstrap today (verified by
`tests/conformance/features/085-method-call-dotted-receiver/source.ev`):

- Field access `obj.field` (Token::Dot, `atoms.rs:20`).
- Dotted-receiver method call `obj.field.method(args)` (composition
  mechanism #6).
- `subclaim Name(args)` inside a parent claim body (parser/schema.rs).
- Subclaims access parent variables via the standard subschema-inline
  pass.
- `is_init` already exists on the FTI for first-tick gating.

So the architecturally-right Stack FTI looks like:

```evident
claim Stack(base ∈ Int, depth ∈ Int, is_init ∈ Bool,
            install_effects ∈ Seq(Effect), ...)
    subclaim BuildAlloc(eff ∈ Effect)
        eff = LibCall("libc", "malloc", ⟨ArgInt(1024)⟩)
    subclaim BuildStore(slot ∈ Int, value ∈ Int, eff ∈ Effect)
        eff = LibCall("__mem", "write_long",
                      ⟨ArgInt(base + slot * 8), ArgInt(value)⟩)

    alloc_eff ∈ Effect
    BuildAlloc(eff ↦ alloc_eff)
    install_effects = (is_init ? ⟨alloc_eff⟩ : ⟨⟩)
```

Host: `effects = main_part ++ my_stack.install_effects`, and
`my_stack.BuildStore(slot ↦ 0, value ↦ 42, eff ↦ store_eff)`.

User framing:

> *"We have `is_init` already... I don't know that we need an
> `install` keyword, we already have `is_init`. Do we support
> references to sub-claims like `my_queue.BuildStackAlloc`? It
> would be more okay if we could at least nest them, and the
> sub-claims get access to the parent claim's variables."*

Yes to all three. No language change needed.

**Why deferred anyway (the unfortunate part):** the self-hosted
`compiler.ev` does NOT yet handle `subclaim` declarations OR
dotted-receiver method calls. Both are in the survey's wave 4. If we
refactor the FTI to use the cleaner shape today:

- Bootstrap compiles it fine (these features have always existed in
  bootstrap).
- After bootstrap is deleted and we recompile with
  `kernel + compiler.smt2`, the FTI re-compile FAILS until wave 4
  lands subclaim + dotted-receiver in compiler.ev.

So adopting the cleaner shape pulls subclaim + dotted-receiver from
"wave 4, post-deletion" into "wave 4, must land BEFORE we can delete
bootstrap." That stretches the deletion path. The user explicitly
chose not to: *"I think, unfortunately, it is not a necessary change.
I am saddened."*

**Status: DEFERRED — post-bootstrap-deletion polish.** The cleaner
shape is exactly right; the parts all exist; we're deferring purely
to keep the deletion path tight. When bootstrap is gone and wave 4 is
done (subclaim + dotted-receiver in compiler.ev), this refactor is
~1 session of work in `stdlib/fti/{stack,queue}.ev` + their fixtures.

The current convention-based shape (host composes `BuildStackAlloc`
via names-match and threads `alloc_eff` into its own `effects`
literal) keeps working through bootstrap deletion and beyond. It's
not pretty but it's correct.

## `Seq(T)` as interface, multiple backings (the real design direction)

**Source:** user, mid-session ~task #33.

**The reframe:** `Seq(T)` should be the language-level surface
syntax (`++`, `#`, `xs[i]`, `⟨…⟩` literals) — semantically a
sequence interface. The choice of underlying SMT-LIB encoding is
a backend decision the compiler makes based on usage, not a
language-level distinction the user has to make.

User rationale:

> *"I really do want to be able to use Seq, it is much cleaner
> than all the alternative structures. If the usage patterns
> (constraint expectations) are satisfying similar to something
> that's more performant, then we should use that. ... this may
> be a naming problem, and a syntax convention problem. We could
> possibly make new FTI's that wrap Cons but call themselves Seq.
> Or a brand new term that actually describes what we're trying
> to do."*

**Candidate backings:**

| Backing | Z3 form | When to use | Cost |
|---|---|---|---|
| Z3-native `Seq(T)` | `(seq.++ (seq.unit a) …)` | Unbounded length truly needed; rare | Z3 sequence theory is slow on real workloads |
| Array+len | `(Array Int T)` + `__len` | Effects channel (kernel reads this); bounded indexable arrays | What bootstrap chose; OK perf but not great |
| Cons cells | `enum SeqOfT = SCons(T, SeqOfT) \| SNil` | Functionize-friendly recursion; AST work-stacks | Currently used under "cons-list" naming |
| FTI-backed | `Int handle` + libc memory | Unbounded streaming, accumulators | Stack/Queue FTI pattern from task #23 |
| Future: hash table backing | TBD | Random access by key | TBD |

**This is essentially a Z3 tactic conceptually** — a tactic
rewrites equivalent symbolic forms for solver efficiency. Picking
the right backing per-Seq is the same operation, applied during
compilation rather than during solving.

**Why this matters for deletion:** the immediate stumbling block
that kept wave 4b from being deletion-ready is exactly the
"different encodings, kernel only accepts one" issue. The deeper
fix is making the choice explicit and the kernel flexible. The
short-term fix (pick one, port it) gets us deleted; the long-term
fix is this design.

**Phases when picked up:**

1. **Categorise existing usage** — every `Seq(T)` in
   `compiler/`, `stdlib/`, `tests/`, `bootstrap/` — what kind of
   sequence is it semantically? Mark with usage hints.
2. **Build a per-backing translator pass** in `compiler/` — one
   per implementation. Each takes Seq AST and emits its specific
   SMT-LIB form.
3. **Either tag explicitly** (`Seq(Effect, encoding=ArrayLen)`) or
   **infer from usage** (effects channel → Array+len;
   work-stack → Cons; etc.).
4. **Add kernel-side decoders** for each backing that needs them.
   Many backings (Cons, Array+len, Z3-native Seq) the kernel
   already handles natively as datatype state.
5. **Benchmark every backing on real workloads** — the cons→Seq
   sweep deferred (`docs/plans/ideas.md` §"Replace Cons-lists with
   Seqs") becomes: "rewrite to whichever Seq-backing wins the
   benchmark for that usage."

**When to pick this up:** post-bootstrap-deletion. The
short-term answer is "match what the kernel expects today
(Array+len for `effects`)" so we can delete. The long-term
answer is this design.

This consolidates the now-deferred entries above (cons→Seq
sweep, FTI honesty) — they're all instances of "we picked one
backing for performance; we should have an abstraction over
backings."

## (Add more ideas here as they surface)
