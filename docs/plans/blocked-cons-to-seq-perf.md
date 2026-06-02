# BLOCKED (perf): cons→Seq sweep of the translator work-stacks (task #21)

**Status:** the Seq-as-state **carry capability is LANDED and correct**
(the foundation gap the prior run of this task documented in
`blocked-cons-to-seq-sweep.md` is now closed). The remaining blocker is
**performance, scoped to the push-heavy translator work-stacks**: carrying
those as `Seq(WorkItem)` is ~250× slower on Z3 than the cons-list they
replace. Per acceptance #6 ("don't ship a perf regression"), the work-stack
`.ev` conversions are **not** shipped; the cons-lists stay. The capability
ships and is covered by `tests/kernel/test_seq_carry.ev`.

## What landed (the toolchain capability)

A `Seq(T)` can now be carried as evolving FSM state across ticks — the three
gaps the prior run identified are all closed:

1. **bootstrap `discover_state_fields`** (`emit.rs`) now emits a `Seq(Elem)`
   state field (`SeqVar` → `Seq(Int|Bool|String)`, `DatatypeSeqVar` →
   `Seq(<type>)`), and `render_prev_state_decl` declares the `_<name>`
   carry-dual correctly as `(Array Int Elem)` + a `_<name>__len` Int (not the
   bogus `Seq(Elem)` sort the naive path would have emitted).
2. **The carry-assignment translates.** `translate_seq_value` (in
   `translate/exprs/seq_eq.rs`) lowers a Seq-valued *expression* to its
   `(array, len)` dual for the shapes a carry needs — `Identifier`,
   `Ternary` (guarded), `seq_init(s)` (drop-last = same array, `len-1`), and
   `base ++ ⟨e0,…⟩` (symbolic-index stores). The `translate_seq_rhs_eq`
   fallback equates the whole Z3 array **and** the `__len` const, which works
   with a *symbolic* length (the elements past `len` are copied as harmless
   garbage the kernel never reads).
3. **The kernel reads + pins it.** `kernel/src/tick.rs` `read_state_var`
   recognises `Seq(Elem)` and reads the array + `__len` (`read_seq_var`,
   tolerant of an unconstrained/dropped Seq → empty); `emit_state_pin` pins
   `_<name>__len` and one `(select _<name> i)` per element.

Proven end-to-end (`tests/kernel/test_seq_carry.ev`, green in all three
functionizer modes): a `Seq(Int)` LIFO stack pushes (`++ ⟨v⟩`), pops
(`seq_init`), indexes from the end (`_stack[#_stack-1]`), and exits — the
exact carry shape the work-stacks need, at < 0.01 s.

## The perf wall (why the work-stacks are not converted)

Converting `tests/kernel/test_translate_arith_recursive.ev` +
`compiler/translate_arith.ev` to `Seq(WorkItem)` produces **byte-identical
output** (`(+ (* 1 2) 3)`) in all three modes — it is correct. But:

| version            | Z3 path (`EVIDENT_FUNCTIONIZE=0`) | default |
| ------------------ | --------------------------------- | ------- |
| cons-list (`WorkList`) | **0.02 s**                    | 0.16 s  |
| `Seq(WorkItem)`        | **5.1 s**                     | 5.1 s   |

~250× on the pure-Z3 path. (Neither version functionizes — the functionizer
already refuses the work-stack via the `DT_IS`/symbolic-index shapes and runs
Z3 — so this is a Z3-vs-Z3 comparison, not a functionizer regression.)

**Root cause:** the binop *expansion* (`new_stack = rest ++ ⟨7 items⟩`) lowers
to a **symbolic-index array store-chain** (`(store rest_arr (+ rest_len k) …)`)
combined with **whole-array extensional equality** (`(= stack_arr expansion_arr)`,
the only length-agnostic way to define the new stack at *translate* time, when
the length is symbolic). Z3's array theory handles `select` well but
extensional array equality + symbolic store indices over a *datatype-valued*
array (`(Array Int WorkItem)`, `WorkItem` wrapping a recursive `Expr`) poorly.
The cons-list does the same reshaping as a single **algebraic-datatype
constructor** (`WLCons(…, rest)`) — cheap, structural — which is exactly why
`architecture-invariants.md` §"Functionizability…" already recommends
cons-lists for AST work-stacks.

Isolation that pins the cause: a `Seq(WorkItem)` that only **pops** (single
drop per tick, no multi-push) runs in 0.01 s; the regression appears only with
the multi-item symbolic-index **push**. The `Seq(Int)` capability fixture
(append + single-pop) is likewise 0.01 s. So append-light / streaming Seq state
is cheap; push-heavy work-stacks are not.

The FTI bodies (`stdlib/fti/{stack,queue}.ev`) hit the same wall harder: their
entire `legal_noop ∨ legal_push ∨ legal_pop` contract is Seq equality inside a
disjunction (`seq_init(contents) = prev`, …), which forces extensional
array-equality case-splits every tick. They were not converted.

## The fix (for a future session)

Two routes, in preference order:

1. **Cons-cell-backed `Seq` lowering with FRONT-pop (recommended).** Carry a
   `Seq(T)` *internally* as the `__SeqOf_T` cons-cell datatype the runtime
   already uses for Seq payloads nested in datatypes
   (`kernel::tick::decode_libargs`). Carry is then **free** — it reuses the
   existing enum-datatype state path — and as fast as today's cons-list,
   because it *is* a cons-list. The work is the surface ops: lower `xs[0]`
   to the head accessor, a `seq_tail`/`seq.extract(xs,1,…)` to the tail
   accessor, `⟨items⟩ ++ xs` to prepend (nested constructors), and `#xs = 0`
   to the empty recognizer (full `#` length stays O(n)/needs a recursive
   `define-fun`, so prefer front-pop shapes that avoid it). This honours the
   user's "Seq surface, even via rewrite rules" intent while keeping perf —
   the source shows `Seq`, the model is a cons-cell. It is a parallel Seq
   lowering in `translate/`, selected for carried datatype-Seqs.

2. **Functionizer/extractor support for symbolic-index arrays.** Teach
   `kernel/src/functionize/` to capture a `Seq` whose length/indices are
   symbolic (today it returns `None` and the whole tick falls to Z3 — see
   `architecture-invariants.md` §"Functionizability…"). Then the array+len
   Seq that landed here would functionize the work-stack the way the cons-list
   doesn't even today, and the 5 s Z3 cost becomes a constant. Larger, more
   speculative.

## Scope decision recorded here

- **Kept the array+len Seq-carry capability** (kernel + bootstrap) — correct,
  validated, cheap for streaming/append-light Seq state, and the genuine
  "Seq carry" unblock the prior run asked be sequenced before any sweep.
- **Did NOT convert** `compiler/parser.ev` (`WorkList`), the 5
  `translate_*.ev` passes, the 6 work-stack fixtures, or the two FTIs — they
  regress past acceptance #6's 2× gate. Cons-lists remain for the work-stacks;
  this is consistent with `architecture-invariants.md`'s standing guidance
  that AST work-stacks belong as cons-lists *for the functionizer*, now with
  an additional Z3-solve-cost reason for the symbolic-index push shape.

## Reproduction

```
cd <repo>
(cd bootstrap/runtime && cargo build --release)
(cd kernel && cargo build --release)
EV=$(echo bootstrap/runtime/targe?/release/evident)
KERNEL=$(echo kernel/targe?/release/kernel)

# Capability (cheap, ships green):
$EV emit tests/kernel/test_seq_carry.ev main -o /tmp/sc.smt2
grep state-fields /tmp/sc.smt2          # → … stack:Seq(Int) …  (carried!)
$KERNEL /tmp/sc.smt2                      # → 7 / 6 / 5, exit 0, < 0.01 s

# The regression (NOT shipped — convert arith to Seq(WorkItem) to repro):
#   replace WorkList→Seq(WorkItem), WLCons-expansion→`rest ++ ⟨…reversed…⟩`,
#   match-pop → `_stack[#_stack-1]` / `seq_init(_stack)`.
# Emits byte-identical "(+ (* 1 2) 3)" but the per-tick Z3 solve is ~250× slower.
```

## Citations

- `tests/kernel/test_seq_carry.ev` — the landed capability, all 3 modes green.
- `tests/kernel/test_functionizer_seqs.ev` (task #19) — the *recomputed* (not
  carried) record-Seq that #19 unblocked; distinct from carry.
- `docs/plans/architecture-invariants.md` §"FTI vs in-Evident cons-list state"
  and §"Functionizability over Z3-fast" — the standing cons-list-for-work-stacks
  guidance this confirms (now with a Z3-solve-cost reason, not only a
  functionizer one).
- `docs/plans/blocked-cons-to-seq-sweep.md` — the prior run's *foundation*
  blocker (now closed by the capability that landed here).
