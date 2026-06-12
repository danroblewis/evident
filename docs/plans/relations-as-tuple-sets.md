# Relations as sets of tuples: dispatch without control flow

**Status:** design proposal (2026-06-12). The set-theoretic successor to
`match` / ternary / `ite` chains. Sibling of `claims-as-sets.md` (that one
is intensional sets; this is their relational/extensional face) and the
construction-side of `model-diff` (which already compares solution sets).

## Thesis

A relation is **a set of (input, output) tuples**. Binding the input
columns and asking the solver for the rest *is* the lookup. So the dispatch
constructs that make the compiler ugly are all the same thing in disguise:

- a `match` is a set of `(pattern, result)` tuples,
- a ternary chain is a set of `(condition, value)` tuples,
- an `ite` spine is a set of `(guard, value)` tuples.

Written as a tuple-set, the control flow disappears and only the data
remains; the solver does the dispatch by membership. The keystone (from an
earlier session) is a mapping written as its own graph:

```evident
(key, val) ∈ {
    ("asdf", "qwer"),
    ("foo",  "bar"),
    ("baz",  "caq")
}
```

Bind `key`, and the only tuple whose first column matches determines `val`.
That is a function written as a set. No `match`, no arrows, no patterns.

## Worked example — a real ugly file

`compiler2/driver.ev` maps a small-integer *sort code* to a Z3 sort handle.
Today it is a 13-arm ternary spine (lines ~895-908):

```evident
sort_h ↦ (new_const_sort = 1 ? z3sorts.bsort
    : new_const_sort ≥ 30 ? ints[1]
    : new_const_sort = 3 ? z3sorts.ssort
    : new_const_sort = 4 ? result_sort
    : new_const_sort = 5 ? int_array_sort
    : new_const_sort = 6 ? z3sorts.rsort
    : new_const_sort = 7 ? effect_sort
    : new_const_sort = 10 ? recs[0].asort
    : new_const_sort = 11 ? recs[1].asort
    : new_const_sort = 12 ? recs[2].asort
    : new_const_sort = 20 ? recs[0].ssort
    : new_const_sort = 21 ? recs[1].ssort
    : new_const_sort = 22 ? recs[2].ssort
    : z3sorts.isort)
```

It is *entirely* a lookup table — the `? :` spine is noise. As a relation:

```evident
-- the point table: a sort code maps to a sort handle
(new_const_sort, sort_h) ∈ {
    (1,  z3sorts.bsort),
    (3,  z3sorts.ssort),
    (4,  result_sort),
    (5,  int_array_sort),
    (6,  z3sorts.rsort),
    (7,  effect_sort),
    (10, recs[0].asort),  (11, recs[1].asort),  (12, recs[2].asort),
    (20, recs[0].ssort),  (21, recs[1].ssort),  (22, recs[2].ssort)
}
```

…with the two non-point arms stated as what they are — a *range* rule and a
*default* (the totality clause):

```evident
new_const_sort ≥ 30                        ⇒  sort_h = ints[1]
new_const_sort ∉ keys ∧ new_const_sort < 30 ⇒  sort_h = z3sorts.isort
```

The table is the whole point; the range and default are two honest extra
rules instead of two ternary arms hiding among twelve. You can *read* the
sort-code map now — it's data, aligned in columns, not a control-flow spine.

## The catch, and the resolution: surface vs lowering

There is a real reason that spine exists: **it functionizes.** A covered
ternary/select chain is exactly the functionizer-safe form (CLAUDE.md, "outputs
must be COVERED"). A naive `(code, sort_h) ∈ {…}` set-membership would go
**residual** — Z3 set theory every tick — and the hot loop would die. So the
relational form cannot be the *lowered* form.

It is the **surface.** The relation lowers *back* to the covered ternary
chain — pretty source, fast artifact — by a pre-oracle transform, exactly as
`lower-bounded-seq` already lowers a keyed-projection pair into a covered
select chain. The ugliness doesn't vanish; it **relocates** from the source
(where a human reads it) to the generated SMT (where no one does). That's the
"keep the surface, change the lowering" rule applied to dispatch.

Two things make this safe:

1. **Totality is checkable.** A point table + a range + a default is a
   *partition* of `new_const_sort`'s domain. The transform can assert the
   default covers exactly the gap (coverage) and the keys don't overlap the
   range (disjointness) — the set identities from `claims-as-sets.md`. An
   under-covered table (a code with no handle) becomes a compile error, not a
   silent wrong sort.
2. **The lowering is verifiable.** `model-diff` proves the relation and the
   ternary chain have the same solution space over `(new_const_sort, sort_h)`.
   Write the relation, lower it, and `model-diff` certifies the lowering
   changed nothing — the regression oracle for this whole refactor.

## Grammars are the recursive case

A token class or nonterminal is a relation between a surface and a value:

```evident
Digit  =  { "0", "1", "2", "3", "4", "5", "6", "7", "8", "9" }
Digits =  Digit  ∪  { d ++ ds | d ∈ Digit, ds ∈ Digits }      -- one-or-more, recursive

Token  =  { ("+", Plus), ("(", LParen), (")", RParen) }        -- finite dispatch
       ∪  { (s, Num(n)) | s ∈ Digits, n = ⟦s⟧ }                -- the numeric family
```

Lexing is `(chars, tok) ∈ Token`; parsing is `(tokens, tree) ∈ Expr`. The
finite parts are enumerated tuples (gorgeous); the repetition/recursion are
recursive set definitions (Kleene closure); the **sequencing becomes explicit
concatenation** (`s = a ++ ⟨Plus⟩ ++ b`) — the one place pure set theory is
wordier than BNF, because BNF *hid* the concatenation. (Concatenation is
`++`, Evident's existing operator; `·` is composition — not this.)

## The unification

Mappings, dispatch, and grammars are now **one mechanism**: relations — sets
of tuples — solved by membership with some columns bound. That is constraint
logic programming / Datalog with Z3 doing the per-tuple constraints. The
runtime stops being "a tick machine, a parser, and a match-evaluator" and
becomes a relational membership solver. One idea retires the keyword table,
the lexer, the parser, and every `match`/ternary in the compiler.

## Honest costs

- **Sequencing is explicit concatenation** — verbose for grammars (none for
  flat dispatch, which is most of the ugly code).
- **Infinite relations are intensional** — `Digits`, `Expr` are never
  enumerated; the solver answers membership symbolically (the lazy-set idea
  from `claims-as-sets.md`).
- **Recursion needs guardedness** to terminate (`guarded-cycles-expressibility.md`).
- **Performance requires the lowering** — the relation *must* compile to the
  covered chain to stay on the functionizer; a relation that can't be lowered
  to a covered form stays residual. The transform, not the surface, is where
  the perf lives.

## Where to start

The **dispatch tuple-relation is the prize**, not the grammar — because it's
the thing that retires the `match`/ternary/`ite` clusters the
`ternary-refactor-census.md` already inventoried (the Group-B sort-code/
sort-handle tables especially), in the set-theory notation the project has
wanted (`no-ternary-chains-preferred`). Grammars are the recursive extension,
to be taken on once the sequencing-as-concatenation cost is judged worth it.

Sequence: (1) define the relational-dispatch surface + its lowering to the
covered chain; (2) gate every rewrite with `model-diff`; (3) convert the
census's Group-B cluster first; (4) revisit grammars.
