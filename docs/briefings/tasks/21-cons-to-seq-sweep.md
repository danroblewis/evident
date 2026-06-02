# Task: Cons→Seq sweep

## Authorisation + why

User authorised this as #2 in the 4-task queue:

> *"Plan to do those in order."*

The functionizer's `recompose_record_seqs` extension landed in
task #19, so a bounded `Seq(Record)` now functionizes at parity
with a cons-list. The architectural preference is Seqs (more
constraint-natural; cons-lists are imperative-shaped). With the
perf reason to defer gone, the sweep is on now.

User framing:

> *"I don't like using the Cons things because we never seen Cons
> in constraint system models. Seq made more sense, and I would
> like to see if we can replace Cons with Seq, even if it has to
> be some rewrite rules."*

## Scope

Replace cons-list-based state in the **codebase under active
construction** with Seq-based state. Specifically:

1. `compiler/parser.ev` — the `WorkItem`/`WorkList` enums driving
   AST work-stacks in translator passes.
2. `compiler/translate_arith.ev`, `_bool.ev`, `_match.ev`,
   `_seq.ev`, `_quant.ev` — the 5 recursive translator passes that
   use `WLCons`/`WLNil` to drive depth-first walks.
3. `stdlib/fti/stack.ev`, `stdlib/fti/queue.ev` — the FTI bodies
   that carry `IntStack`/`IntQueue` cons-lists.
4. `tests/kernel/test_translate_*_recursive.ev`,
   `test_fti_stack.ev`, `test_fti_queue.ev`, `test_ast_walker.ev`,
   any other fixture exercising those passes.

**Out of scope** (do NOT touch):

- `bootstrap/` (frozen reference).
- `compiler/lexer.ev` — uses primitive char-by-char state, no
  cons-list.
- Tests under `tests/conformance/features/` — these are
  language-spec contracts, not implementation patterns.
- `compiler/compiler.ev` — the MVP driver; it composes the above
  passes, so the changes percolate, but its own logic shouldn't
  need an independent cons→Seq rewrite. Touch only if your
  sweep breaks its current behavior.

## Required reading

1. `CLAUDE.md`.
2. `docs/plans/architecture-invariants.md` — note the recently-added
   "Seqs are the destination shape" guidance (post task #19); the
   "TRANSITIONAL — cons-lists are an expedient" flag should now
   come OFF after this sweep.
3. `docs/plans/ideas.md` §"Replace Cons-lists with Seqs" — note the
   PARTIALLY COMPLETE status; this task completes steps 2–4.
4. `docs/plans/functionizer-integration.md` §6 (LANDED record-Seq
   recomposition) — what the kernel can functionize now.
5. `tests/kernel/test_functionizer_seqs.ev` — the worked example of
   a Seq-of-records FSM. Mirror its shape.
6. `tests/kernel/test_translate_arith_recursive.ev` — the worked
   example of the cons-list pattern you're replacing. Compare
   side-by-side.
7. `stdlib/fti/stack.ev` + `tests/kernel/test_fti_stack.ev` — same.

Cite #5 and the specific cons-list files you rewrote.

## Translation table (likely shape)

```
ENUM CONS                              SEQ EQUIVALENT
─────────────────────────              ────────────────────────────
enum WorkList = WLNil                  -- (no separate enum needed)
              | WLCons(WorkItem, ..)
                                       work ∈ Seq(WorkItem)

state-carry:                           state-carry:
  _work ∈ WorkList                       _work ∈ Seq(WorkItem)
  work = WLNil (init)                    work = ⟨⟩ (init)

push x:                                push x:
  work = WLCons(x, _work)                work = _work ++ ⟨x⟩

pop (get head + tail):                 pop (get tail + remainder):
  match _work                            head ∈ WorkItem = _work[0]
    WLCons(h, t) ⇒ … h, t                rest ∈ Seq(WorkItem) =
                                           seq.extract(_work, 1,
                                                       #_work - 1)
```

The "pop" form differs: cons-lists are head-prepended LIFO; Seqs
with `++ ⟨x⟩` append. Match the *semantics* of the existing
pass (most use the work-stack as a LIFO traversal stack), so the
Seq version probably pops from the END:

```
top ∈ WorkItem = _work[#_work - 1]
rest ∈ Seq(WorkItem) = seq.extract(_work, 0, #_work - 1)
```

Verify each pass's traversal order is preserved by re-running its
recursive test — the SMT-LIB output should be byte-identical.

## Acceptance

1. Each of `parser.ev`'s `WorkList`/`WLCons`/`WLNil` references is
   gone; `Seq(WorkItem)` is used instead. The `WorkItem` enum
   itself stays (it's the element type, not the list).
2. Each of the 5 recursive translator passes uses
   `Seq(WorkItem)` state, not cons-lists.
3. `stdlib/fti/stack.ev`'s `IntStack` enum is gone; `Seq(Int)` is
   used. Same for `IntQueue` in `queue.ev`.
4. All affected test fixtures still pass and emit the same
   SMT-LIB / stdout / exit as before.
5. `./test.sh` is fully green in all 3 modes (default,
   `EVIDENT_FUNCTIONIZE_JIT=0`, `EVIDENT_FUNCTIONIZE=0`).
6. **Benchmark check** — run `tests/kernel/test_translate_arith_recursive.ev`
   under the functionizer (default mode) and confirm the
   per-tick time is within 2× of pre-sweep. If the Seq-based
   version is materially slower, document why and either:
   - Add the Seq-step recomposition the recompose_record_seqs path
     should handle, OR
   - Write `docs/plans/blocked-cons-to-seq-perf.md` and pause —
     don't ship a perf regression.
7. `docs/plans/architecture-invariants.md` updated: remove the
   "TRANSITIONAL — cons-lists are an expedient" section; replace
   with "Seqs are the standard shape." Cons-lists may remain as
   *element* enum types (e.g. for `Expr` AST nodes); they should
   not appear as state-carry container types.
8. `docs/plans/ideas.md` §"Replace Cons-lists with Seqs" updated:
   mark COMPLETE; leave a one-line history.

## Authorisation update (post-session-21 finding)

The previous run of this task discovered that the cons→Seq sweep is
blocked on **three toolchain gaps**, not just the .ev rewrites:

1. `bootstrap/runtime/src/emit.rs:515-516` (`discover_state_fields`)
   drops Seq/Set/Composite state fields — the manifest won't
   include them.
2. The carry assignment `work = (is_first_tick ? ⟨⟩ : _work)`
   doesn't translate (Seq-var equality + ternary-over-Seq gap).
3. `kernel/src/tick.rs`'s `Sv::Seq.smtlib()` is `unreachable!()`.

The user explicitly authorised editing all three:

> *"Why would it involve editing anything in bootstrap/? ... If
> [we have to use bootstrap to compile], then for that session it
> should be allowed to modify the bootstrap code."*

So for THIS session: bootstrap edits ARE in scope, scoped to
exactly what's needed to make `Seq(...)` work as state-carry.
Specifically:

- `bootstrap/runtime/src/emit.rs::discover_state_fields` — extend
  to include Seq fields with their element-type rendered into the
  manifest type string.
- Wherever bootstrap's translate path handles state-carry
  assignment `(is_first_tick ? init : _x)` — extend to handle
  Seq-typed `x`.
- `kernel/src/tick.rs` — implement `Sv::Seq.smtlib()` (pin form
  + read-back from the model).

Bootstrap is reference material; we're editing it because it's
on the path until `compiler.ev` matures. The edits are throwaway
when bootstrap is deleted; that's OK.

## Forbidden

- Editing the `WorkItem` element enum (it's the payload, not the
  list).
- Editing `compiler/lexer.ev` (no cons-list there to rewrite).
- Editing `tests/conformance/features/` (language-spec contracts;
  unchanged).
- Adding Python.
- Working around perf regressions by reverting partial passes;
  either land the whole sweep clean or document a blocker.
- Bootstrap edits BEYOND what the three gaps above require. Keep
  the bootstrap delta minimal — it's destined for deletion.

## Reporting back

- Branch pushed (`agent-21-cons-to-seq-sweep` or similar).
- Files modified (paths only).
- One sentence per affected pass: "translate_arith: cons→Seq
  clean, 5 fixtures green, perf X ms" — confirms each is good.
- `./test.sh` final line, all 3 modes.
- Per-fixture per-tick time before/after (from
  `tests/kernel/test_translate_arith_recursive.ev` etc.) — the
  perf gate from acceptance #6.
- Cite #5 and the cons-list files you rewrote.

If you can't keep perf within 2×: write
`docs/plans/blocked-cons-to-seq-perf.md` describing which pass
regressed, by how much, and what the functionizer or extractor
needs.
