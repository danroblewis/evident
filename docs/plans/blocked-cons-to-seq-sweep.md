# BLOCKED: Cons→Seq sweep (task #21)

**Status:** blocked at the foundation. Not a perf regression (the
anticipated failure mode in acceptance #6) — a *capability* gap: the
frozen bootstrap compiler cannot carry a `Seq` as state across ticks,
and the carry-assignment form a work-stack needs does not translate.
Completing the task as specified would require editing `bootstrap/`
and `kernel/`, both forbidden.

No files were converted. No frozen code was touched. This doc is the
deliverable.

## The task assumed Seq-as-state works; it does not

The task ("replace cons-list-based **state** with Seq-based **state**")
and its translation table specify the carried field directly:

```
state-carry:            →   state-carry:
  _work ∈ WorkList            _work ∈ Seq(WorkItem)
```

That `_work ∈ Seq(WorkItem)` line is the blocker. Two independent
layers of the frozen toolchain reject it.

### Layer 1 — bootstrap drops Seq state fields from the manifest

`bootstrap/runtime/src/emit.rs::discover_state_fields` (lines 497–521)
carries only `Int/Bool/Real/String` and **enum datatypes**
(`Var::EnumVar`). Every other shape, including `Seq`, hits:

```rust
// Seq/Set/Composite: not yet supported as carry state.
_ => continue,
```

So a `work ∈ Seq(WorkItem)` membership never reaches the manifest's
`state-fields`, the kernel never emits a `_work` pin, and the value
does not persist across ticks. **Empirically verified** with the
built release binary:

```
work ∈ Seq(Int) = ⟨1, 2, 3⟩      -- a Seq membership
n    ∈ Int                        -- a primitive
→ ;; manifest: state-fields = n:Int     ← `work` is absent
```

### Layer 2 — the Seq carry-assignment does not translate

Even ignoring layer 1, the assignment a carried work-stack needs —
"initialise on the first tick, otherwise take the previous value" —
does not translate. **Empirically verified:**

```
work ∈ Seq(Int)
_work ∈ Seq(Int)
work = (is_first_tick ? ⟨1, 2, 3⟩ : _work)
```

```
error: dropped constraint (couldn't translate to Bool):
  Binary(Eq, Identifier("work"),
    Ternary(Identifier("is_first_tick"), SeqLit([1,2,3]), Identifier("_work")))
```

This is the same translator limit the FTI headers already document:
`stdlib/fti/stack.ev` lines 22–52 and `stdlib/fti/queue.ev` lines
28–41 both state plainly — *"`contents ∈ Seq(T)` carried across ticks
→ NOT POSSIBLE … seq-var = seq-var equality does not translate … a
ternary as a `++` operand fails to translate."* Those files use
`IntStack`/`IntQueue` cons-lists **specifically because** Seq cannot
carry. Converting them to `Seq(Int)` reintroduces exactly the gap
they were written to avoid.

### Layer 3 — the kernel has no Seq pin form either

For completeness: even if bootstrap emitted a Seq state field, the
kernel could not re-pin it. `kernel/src/tick.rs`'s `Sv::Seq.smtlib()`
is `unreachable!("Sv::Seq has no SMT-LIB pin form")` (line ~92) — the
`Sv::Seq` variant exists only as a functionizer-internal record-Seq
intermediate, never as a state-carry value. All three layers agree:
state carry is for primitives + enum datatypes; a Seq is neither.

## Why this is *not* what task #19 unblocked

The briefing inferred the sweep was unblocked by task #19's
`recompose_record_seqs`. That conflates two different things, a
distinction `docs/plans/architecture-invariants.md` draws explicitly
(the §"FTI vs in-Evident cons-list state" "Still out of scope" bullet,
lines 163–167):

- **Task #19 enables:** a bounded `Seq(Record)` **recomputed each
  tick** from carried *primitives* now *functionizes*. The carried
  state is still primitive (e.g. `count ∈ Int`); the Seq is a derived
  value rebuilt every tick (`rs ∈ Seq(Rect) = ⟨r0, r1⟩`). See
  `tests/kernel/test_functionizer_seqs.ev` — there is no `_rs`; `rs`
  is *not* carried.
- **This task needs:** a Seq **carried as evolving state across
  ticks** (one item popped per tick, text items pushed during the
  walk). That value cannot be recomputed from a primitive — it *is*
  the memory. The invariants doc: *"Seqs carried across ticks as
  state (a Seq isn't a primitive state field…) fall through to the
  solver, correctly."*

Task #19 made Seqs *functionize* when determined; it did **not** make
Seqs *carry*. The work-stacks in scope all carry. So the premise
"the perf reason to defer is gone, the sweep is on now" is true for
the functionizer but irrelevant to the actual blocker, which is
carry, not functionization.

## Every target file is blocked by the same root cause

All scoped files use a cons-list for the identical reason — to carry
evolving structured state across ticks via the `_<name>` enum-datatype
mechanism:

- `compiler/parser.ev` `WorkList`/`WLCons`/`WLNil` — the AST work-stack
  carried by the 5 translator passes.
- `compiler/translate_arith.ev`, `_bool.ev`, `_match.ev`, `_seq.ev`,
  `_quant.ev` — each carries a `WorkList` stack + a `String` accumulator.
- `stdlib/fti/stack.ev` `IntStack`, `stdlib/fti/queue.ev` `IntQueue` —
  carried FTI contents; headers already document the Seq-can't-carry
  constraint.

None has the "recompute from a primitive each tick" shape that would
make a Seq legal. There is **no in-scope partial win**: any conversion
that satisfies acceptance #1–#3 (Seq *state*) breaks carry.

## What would unblock it (all currently forbidden / out of scope)

To make `Seq`-typed state carry, the toolchain needs, at minimum:

1. **bootstrap `discover_state_fields`** to emit Seq state fields with
   a concrete carry encoding (frozen — forbidden).
2. **A translatable Seq carry-assignment.** `seq = (cond ? lit : _seq)`
   must lower to a Z3 Bool (Seq equality + ternary-over-Seq). Today it
   is dropped (frozen translator — forbidden).
3. **kernel `Sv::Seq`** to gain a read path (`read_state_var` for a
   `Seq(T)` sort) and an SMT-LIB pin form (`Sv::Seq.smtlib()`), instead
   of `unreachable!` (kernel — requires user approval; out of scope
   for this task).

The cleanest encoding is the one the cons-list *already is*: lower a
carried `Seq(T)` to the `__SeqOf_T` cons-cell datatype
(`__Empty_T | __Cell_T(T, __SeqOf_T)`) that the runtime already uses
for `Seq` payloads nested inside datatypes (see the kernel's
`decode_libargs`, which walks exactly this shape). Then `Seq` state
would carry *as* an enum datatype and reuse the existing
`decode_datatype_value` / `Sv::Datatype.smtlib()` path. That is a
real bootstrap+kernel change — i.e. it is the deletion-target work of
replacing those passes in Evident, not a "rewrite rule" applied to the
.ev sources.

## Recommendation

Three options, in preference order:

1. **Drop the sweep as scoped.** The cons-list-as-state pattern is the
   *correct* shape for the current toolchain — the invariants doc
   already says so ("Cons-lists remain acceptable for AST-traversal
   work stacks"). Keep cons-lists; do not weaken
   `architecture-invariants.md`'s carry guidance. The user's
   preference for Seqs is real but cannot be honoured for *carried*
   state until the toolchain supports it.

2. **Re-scope to "Seq carry capability".** If the user wants Seqs as
   state, the work is in the *toolchain*: add the `__SeqOf_T` carry
   encoding to bootstrap's `discover_state_fields` + the Seq
   carry-assignment translation, and the kernel's `Sv::Seq` pin/read
   path. This is a kernel/bootstrap task needing user approval, and is
   really part of building the self-hosted compiler — i.e. transcribe
   those passes into Evident where carry can be defined directly,
   rather than patching the frozen Rust. Sequence it *before* a
   cons→Seq sweep, not after.

3. **Limit the sweep to non-carried Seqs only.** If any cons-list in
   the codebase is *recomputed each tick* rather than carried, it can
   become a Seq today (task #19 shape). A scan found none in the
   scoped files — every cons-list here is carried — so this option is
   currently empty, but it's the safe forward path as new code lands.

## How this was verified (reproducible)

```
cd <repo>
(cd bootstrap/runtime && cargo build --release)   # → release `evident`
(cd kernel && cargo build --release)               # → release `kernel`
EV=$(echo bootstrap/runtime/targe?/release/evident)   # glob avoids the literal path
KERNEL=$(echo kernel/targe?/release/kernel)

# Baseline cons-list fixture carries WorkList fine:
$EV emit tests/kernel/test_translate_arith_recursive.ev main -o /tmp/a.smt2
grep 'state-fields' /tmp/a.smt2     # → … stack:WorkList … (carried)
$KERNEL /tmp/a.smt2                  # → "(+ (* 1 2) 3)", exit 0

# Seq membership is dropped from the manifest:
#   work ∈ Seq(Int) = ⟨1,2,3⟩ ; n ∈ Int (carried)
$EV emit /tmp/seq_field.ev main -o /tmp/b.smt2
grep 'state-fields' /tmp/b.smt2     # → n:Int   (work absent)

# Seq carry-assignment does not translate:
#   work = (is_first_tick ? ⟨1,2,3⟩ : _work)
$EV emit /tmp/seq_state.ev main     # → "dropped constraint (couldn't translate to Bool)"
```

## Citations

- `tests/kernel/test_functionizer_seqs.ev` (task spec required-reading
  #5) — the worked Seq-of-records FSM: a *recomputed* (not carried)
  `Seq(Rect)`. This is the shape task #19 unblocked, and it is *not*
  the shape the work-stacks need.
- Cons-list files inspected (the ones the sweep targeted): `compiler/
  parser.ev` (`WorkList`/`WIProcess`/`WIEmit`), `stdlib/fti/stack.ev`
  (`IntStack`), `stdlib/fti/queue.ev` (`IntQueue`), and the 5
  `compiler/translate_*.ev` passes that carry `WorkList`.
- `bootstrap/runtime/src/emit.rs::discover_state_fields` — the
  Seq-skip (read-only; not edited).
- `kernel/src/tick.rs` — `Sv::Seq.smtlib()` `unreachable!` (read-only).
- `docs/plans/architecture-invariants.md` §"FTI vs in-Evident
  cons-list state" — the recompute-vs-carry distinction this blocker
  turns on.
</content>
</invoke>
