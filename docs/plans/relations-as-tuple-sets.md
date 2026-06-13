# Dispatch as set membership: sets of tuples, and nothing new

**Status:** design proposal (2026-06-12, rewritten 2026-06-13 to the minimal
form we converged on). The set-theoretic successor to `match` / ternary /
`ite` chains. Sibling of `claims-as-sets.md` (the set algebra) and the
construction-side of `model-diff` (which already *compares* solution sets).

> This is a clean rewrite. An earlier draft carried scaffolding we then
> discarded in design discussion — a `relation` keyword, "tuple positions are
> sets," mutable/extensible relations. None of that survived. What's left is
> smaller and is recorded here; the discarded ideas and *why* they fell are in
> the "What we tried and dropped" section at the end.

## Thesis

A `match`, a ternary chain, and an `ite` spine are all the same object in
disguise: **a set of (input, output) tuples**. Bind the input column, ask the
solver for the rest, and membership *is* the lookup. Written as a set, the
control flow disappears and only the data remains — the solver does the
dispatch.

The keystone (from an earlier session) is a mapping written as its own graph:

```evident
(key, val) ∈ {
    ("asdf", "qwer"),
    ("foo",  "bar"),
    ("baz",  "caq")
}
```

Bind `key`; the only tuple whose first column matches fixes `val`. A function
written as a set. No `match`, no arrows, no patterns.

## The whole vocabulary — no new nouns

The entire dispatch / dict / mapping / grammar story reuses one set of tools.
Nothing here is a new construct:

| need | tool | already-have? |
|------|------|---------------|
| the table | a **set of tuples** — `Set⟨(String, Int)⟩` | `Set(T)` + a tuple/record element type |
| the lookup | `∈` membership | yes |
| compose two tables / alternatives | `∪` | from `claims-as-sets` |
| dynamic or infinite rows | set-builder `{ x | P }` | from `claims-as-sets` |
| the default | `otherwise` | sugar over the keyed-projection default |
| name the set | `claim` (or `≜`, its synonym) | yes |

There is **no** `relation` keyword, no `dict`, no `grammar` — a relation is
just a `Set` whose elements are tuples; the rest is set algebra. "Dispatch
table," "mapping," and "grammar rule" are the same object used three ways.

## Positions hold elements, not sets

A tuple position is a single **element** (a literal or a variable) — the two
columns of the sort table are both `Int`, always. The "set" that gives a row
its reach is the **solver's search space for a variable**, not anything sitting
in the tuple. A row's key can be a *constrained variable*, and the solver binds
it to match the lookup key, searching its domain:

```evident
(new_const_sort, sort_h) ∈ {
    (1, bsort),  (3, ssort),  (4, result_sort),  …,
    (x ∈ { k | k ≥ 30 }, ints[1])      -- x is an Int; the constraint restricts which
}  otherwise  isort
```

`1`, `3`, `x` are all `Int` elements — no type mixing. The row `(x, ints[1])`
matches when `new_const_sort` unifies with some `x ≥ 30`, i.e.
`new_const_sort ≥ 30`. A many-to-one row is the same shape with a set-valued
*domain* on its key-variable:

```evident
Numeric ≜ { "Int", "Nat" }              -- a meaningful group, named once, reused
(x ∈ Numeric, isort_h)                  -- one row covering every value Numeric admits
```

The key-variable is **existentially scoped to the row** — re-bound per lookup —
which is why one row covers a whole group instead of one value. This is plain
logic-variable / CLP semantics: a Datalog rule with a constrained variable; the
solver unifies the key and checks the constraint. (And: an `∈`-constrained
variable is *one searched element*, not a set living in the tuple — the
distinction that an earlier draft got wrong.)

## Worked example — a real ugly claim

`compiler2/driver_record.ev:44` `RtSortOf` maps a typename to a sort. Today: 8
`⇒`-arms plus a 5-line negated-disjunction default that re-lists every condition
by hand just to say "otherwise 0." As a set of tuples:

```evident
claim RtSortOf(typename ∈ String, isort_h, bsort_h, ssort_h, rsort_h, iarr_h ∈ Int, sort ∈ Int)
    (typename, sort) ∈
        {  (x ∈ Numeric, isort_h),                 -- Int / Nat, one row
           ("Bool", bsort_h),  ("String", ssort_h),
           ("Real", rsort_h),  ("Seq(Int)", iarr_h)  }
        ∪  { (r.name, r.sort) | r ∈ recs, r.name ≠ "" }   -- the live record registry
        otherwise  sort = 0
```

14 lines → ~5, and the hand-written negated disjunction collapses into
`otherwise`. You read the sort table instead of parsing a control-flow spine,
and coverage becomes the language's job (an uncovered key is a totality *error*,
not a silent `sort = 0`).

## Composition is immutable union — there is no "add to the set"

To extend a table you do **not** mutate it; you `∪` it with another table to
make a new value. `Numeric`, the registry table, the primitive table are each
closed, fixed values; a lookup composes exactly the ones it needs *at the use
site*:

```evident
(typename, sort) ∈ (Primitives ∪ recordSorts)  otherwise  0
```

This dissolves the hard parts of "extensible dispatch" outright:

- **No open/closed-world leak.** Each piece is a closed set; a union of closed
  sets is closed. (Contrast: asserting `row ∈ S` as a *constraint* only forces
  `S` to be a *superset*, leaving the lookup free to invent rows — unsound. A
  defined set must be **closed-world**: exactly its rows, nothing more.)
- **No construct-vs-use ambiguity.** You *define* closed sets, *compose* with
  `∪`, *query* with `∈`. Three distinct operations.
- **No mutation ordering / stratified defaults.** Compose first, then query, so
  `otherwise` always sees the final set.

The one obligation that remains — because it is inherent to "is this a
function," not a flaw of any approach — is **disjointness**. `A ∪ B` *collects*;
it does not *reconcile*. If two rows map the same key to different values, the
union maps that key to two values and is no longer a function. So a dispatch
table carries a disjointness obligation (the `∩ = ∅` identity) plus coverage
(`⋃ = domain`, or `otherwise`). A ternary spine hides this behind first-match;
the set makes it explicit and **checkable** (`model-diff` / the partition
identities from `claims-as-sets`).

## Closed-world definitions (and why grammars need the same thing)

A set *definition* is read as the **least fixpoint of its rules** — closed to
invention (the solver can't fabricate a row no rule derives), open to extension
(more rules, via `∪`, mean more rows). This is what makes the lookup sound, and
it is the *same* semantics the recursive case needs:

```evident
Digit  ≜  { "0", "1", "2", "3", "4", "5", "6", "7", "8", "9" }
Digits ≜  Digit ∪ { d ++ ds | d ∈ Digit, ds ∈ Digits }     -- least fixpoint = Kleene+
```

So one semantic choice — sets are least-fixpoints of their defining rules —
powers both the dict-then-query dispatch and the recursive grammars
(`Token`, `Expr` as relations of surface↔value). Recursion needs guardedness to
terminate (`guarded-cycles-expressibility.md`); a grammar's sequencing is
explicit concatenation (`++`, Evident's operator — *not* `·`, which is
composition), which is the one place pure set notation is wordier than BNF
because BNF hid the concatenation.

## Surface vs lowering — why this is viable at all

The ternary spine exists for a reason: **it functionizes** (a covered select
chain is the functionizer-safe form; CLAUDE.md "outputs must be COVERED"). A
naive `(key, val) ∈ {…}` membership would go **residual** and kill the hot loop.
So the set of tuples is the **surface**; it lowers *back* to the covered ternary
chain — pretty source, fast artifact — via a pre-oracle transform, exactly as
`lower-bounded-seq` already lowers a keyed-projection pair into a covered select
chain. The ugliness doesn't vanish; it **relocates** from the source a human
reads to the SMT no one does. And `model-diff` certifies the lowering preserves
the solution space over `(key, val)` — the regression oracle for this whole
refactor.

## What's genuinely new (it's small)

- set operators (`∪` / `∩` / `∖`) and set-builder in expression position;
- tuple element types / literals (`(String, Int)`, `(a, b)`) if anonymous
  tuples aren't already covered by records;
- `otherwise` sugar (lowers to the keyed-projection default);
- the **semantic** decision that a set *definition* is closed-world /
  least-fixpoint;
- the **lowering pass** from a tuple-table to the covered select chain.

No `relation`, no `dict`, no `grammar` keyword. Everything else is `Set`, `∈`,
`claim`, and the set algebra.

## Where to start

The **dispatch tuple-set is the prize**, not the grammar — it retires the
`match`/ternary/`ite` clusters `ternary-refactor-census.md` inventoried (the
Group-B sort-code/sort-handle tables especially), in the set-theory notation the
project has wanted (`no-ternary-chains-preferred`). Sequence:

1. define the tuple-table surface + `otherwise`, with closed-world reading;
2. build the lowering to the covered select chain;
3. gate every rewrite with `model-diff`;
4. convert the census's Group-B cluster first;
5. revisit grammars (the recursive case) once the concatenation cost is judged
   worth it.

## What we tried and dropped (so we don't relitigate)

- **A `relation` keyword** — redundant; a relation is a `Set` of tuples.
- **"Tuple positions are sets"** — wrong; positions are elements, and `1` and
  `{k|k≥30}` can't share a column. The reach comes from a *constrained
  variable* whose domain the solver searches.
- **Mutable / self-registering extensible relations** (`SortOf ⊇ row` from many
  modules) — needs contribution-vs-query marking, stratified negation, and
  cross-module conflict policy. All of it dissolves by making tables immutable
  values and composing with `∪` at the use site.
- **`≜` as a distinct binder** — it's a synonym for `claim` (which also carries
  the element-type header `≜` can't). Keep `claim`; `≜` is optional sugar.
