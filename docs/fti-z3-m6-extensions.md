# M6 — set-theoretic and quantifier extensions to Formula

This is a mechanical extension to the Z3 FTI landed in M5. The
`Formula` datatype gains constructors for set-theoretic operations
and quantifiers; the `materialize` function gains corresponding
arms; the Z3 FTI itself doesn't change shape. Read `docs/fti-z3.md`
first — this doc assumes you have that context.

## New Formula constructors

Per `docs/prelude-plan.md` M6, add to the Formula datatype:

```
type Formula =
    ; ...all the M5 constructors, plus:

    ; Set operations
    | SetEmpty(sort_name ∈ String)
    | SetFull(sort_name ∈ String)
    | SetAdd(set ∈ Formula, elem ∈ Formula)
    | SetDel(set ∈ Formula, elem ∈ Formula)
    | SetUnion(l ∈ Formula, r ∈ Formula)
    | SetIntersect(l ∈ Formula, r ∈ Formula)
    | SetDifference(l ∈ Formula, r ∈ Formula)
    | SetComplement(set ∈ Formula)
    | SetMember(elem ∈ Formula, set ∈ Formula)
    | SetSubset(a ∈ Formula, b ∈ Formula)

    ; Quantifiers — body refers to a bound variable named by `var_name`
    | Forall(var_name ∈ String, sort_name ∈ String, body ∈ Formula)
    | Exists(var_name ∈ String, sort_name ∈ String, body ∈ Formula)
```

12 new constructors. Each one needs:
1. A case in the `materialize` recursive function.
2. The corresponding Z3 C-API libcall(s).

## Z3 C-API operations to wrap

| Formula constructor | Z3 C-API function | Sig |
|---|---|---|
| SetEmpty(T) | `Z3_mk_empty_set(ctx, sort)` | `l(ll)` |
| SetFull(T) | `Z3_mk_full_set(ctx, sort)` | `l(ll)` |
| SetAdd(s, e) | `Z3_mk_set_add(ctx, set, elem)` | `l(lll)` |
| SetDel(s, e) | `Z3_mk_set_del(ctx, set, elem)` | `l(lll)` |
| SetUnion(l, r) | `Z3_mk_set_union(ctx, n, args)` | needs n-ary marshaling |
| SetIntersect(l, r) | `Z3_mk_set_intersect(ctx, n, args)` | needs n-ary marshaling |
| SetDifference(l, r) | `Z3_mk_set_difference(ctx, l, r)` | `l(lll)` |
| SetComplement(s) | `Z3_mk_set_complement(ctx, set)` | `l(ll)` |
| SetMember(e, s) | `Z3_mk_set_member(ctx, elem, set)` | `l(lll)` |
| SetSubset(a, b) | `Z3_mk_set_subset(ctx, a, b)` | `l(lll)` |
| Forall(v, T, body) | `Z3_mk_forall_const(ctx, weight, n, bound, n_pat, pat, body)` | complex |
| Exists(v, T, body) | `Z3_mk_exists_const(ctx, weight, n, bound, n_pat, pat, body)` | complex |

## The n-ary union/intersect problem

`Z3_mk_set_union` takes an array of asts and a count. Our current
sig grammar (`i/l/d/s/v`) has no array type — only scalar primitives.

Three options:

**Option N1: Binary chain.** `SetUnion(a, b, c)` is `SetUnion(a,
SetUnion(b, c))` — recursive binarization. Use the binary form even
in the Z3 C API: `Z3_mk_set_union(ctx, 2, [a, b])`. The "n-ary"
function is called with n=2 always.

To pass `[a, b]` as a C array, we need either:
- Allocate via `__mem__`/malloc, write the handles, pass the address as `l`
- Or add a new sig variant for "array of long"

Option N1's binary approach plus malloc+store is what we'd do. Maybe
4-5 libcalls per binary set union: alloc 16 bytes; mem_store_long
each handle; call Z3 with the address; free.

**Option N2: Extend the sig grammar.** Add `L` (capital L) for
"pointer to long array, size in next int arg" or similar. More
surgical but a sig change is a real bug fix.

For v1: pick N1 (binary chain). It's verbose but uses only existing
primitives. ~10 libcalls per binary union vs ~1 if we had array
sigs.

## The quantifier problem

`Z3_mk_forall_const` is genuinely complex — it takes patterns,
weights, bound variables, etc. The simplest invocation:

```c
Z3_mk_forall_const(ctx, 0, 1, &bound_var, 0, NULL, body)
```

Where `bound_var` is a Z3_app (a constant created by `Z3_mk_const`),
and `body` references that constant. Patterns are NULL for v1.

This needs:
1. Marshaling the bound variable (a single `Z3_mk_const` with the
   given sort)
2. Marshaling the body (recursive — the body Formula references the
   bound variable by name, but Z3 references it by AST identity, so
   the materialize machinery has to make this work)
3. Passing the bound variable as a single-element array (needs
   malloc+store like SetUnion)
4. Passing NULL for patterns (a sig `0` arg) — we can pass ArgInt(0)
   if `Z3_mk_forall_const` accepts a null array pointer represented
   as 0

The bound-variable referencing inside the body is the tricky part.
Our `Var(name, sort)` constructor creates a Z3 const via
`Z3_mk_const(ctx, sym, sort)`. Inside a quantifier, the bound var
should reference the SAME AST as the bound declaration.

Two ways:

**Option Q1: Convention.** When materializing a Forall, emit the
bound variable's Z3_mk_const first (push its handle on the
scratchpad with a specific name like `@bound`), then materialize
the body. When the body's `Var(name, ...)` matches the bound name,
ArgRef("@bound") instead of materializing a fresh const.

This requires the materialize function to track "what bound
variables are in scope." The recursive structure of materialize
doesn't naturally do this — Z3 functions don't take a context
argument like that easily.

**Option Q2: Name-based identity.** Z3's `Z3_mk_const` with the
same symbol and sort returns the same AST. So if we materialize
`Var("x", "Int")` inside and outside a Forall, we get the same AST
handle both times — Z3 deduplicates.

If Q2 is true (worth testing), then quantifier materialization is
much simpler: just materialize the bound var and the body
independently, then call `Z3_mk_forall_const` with the bound var
handle and the body handle. The internal `Var("x", "Int")` references
in the body produce the same handle.

Test Q2 in a small Python script before committing to a design.

## materialize function arms

For each new constructor, add a case to the recursive materialize:

```
match f:
    ...all the M5 cases, plus:

    SetEmpty(sort) =>
        materialize_sort(sort)
        ++ [LibCall("libz3", "Z3_mk_empty_set", "l(ll)",
                    [ArgRef("@ctx"), ArgRef("@stack[-1]")],
                    "@push", "")]

    SetAdd(s, e) =>
        materialize(s)
        ++ materialize(e)
        ++ [LibCall("libz3", "Z3_mk_set_add", "l(lll)",
                    [ArgRef("@ctx"),
                     ArgRef("@stack[-2]"),    ; the set
                     ArgRef("@stack[-1]")],   ; the element
                    "@push", "")]
        ; (the @push consumes 2 and pushes 1 net; final stack depth -1)
        ; Wait — pushes don't auto-pop. After the libcall, both args
        ; AND the new result are on the stack. Need to pop the two
        ; arg handles before pushing the result. The runtime needs
        ; a "consume-N-push-1" primitive, or we use a separate
        ; @pop sentinel.

    SetUnion(l, r) =>
        ; Binary chain. Materialize both subtrees, then build the
        ; size-2 array via malloc+store, then call Z3_mk_set_union.
        materialize(l)
        ++ materialize(r)
        ++ [LibCall("__mem__", "mem_alloc", "l(l)",
                    [ArgInt(16)], "@push", ""),       ; arr_ptr
            LibCall("__mem__", "mem_store_long", "v(ll)",
                    [ArgRef("@stack[-1]"),            ; arr_ptr
                     ArgRef("@stack[-3]")],           ; l_handle
                    "", ""),
            LibCall("__mem__", "mem_store_long", "v(ll)",
                    [add(arr_ptr, 8),                 ; arr_ptr + 8
                     ArgRef("@stack[-2]")],           ; r_handle
                    "", ""),
            ; ... this is getting unwieldy ...
            ]
```

I notice a real problem here: the "consume-N-push-1" semantics. The
current `@push` adds to the stack but doesn't pop arguments. After
multiple libcalls, the stack has interleaved results that are hard
to address by index.

**Fix: ok_dest format `"@pop:N:@push"`.** Means "pop N handles from
stack, then push my result." Default `@push` is `"@pop:0:@push"`.
Most materialize cases want `@pop:2:@push` (consume two args,
push one result).

This is another runtime change but small (~5 lines in `_dispatch_effects`
to parse the ok_dest sentinel and call `pop()` N times before pushing).

Alternatively, design materialize so that each subtree's result
stays at the top of stack until the next operation consumes it, and
operations consume their args from the top. This is the natural
RPN evaluation pattern; we just need the right primitive.

## What needs to land for M6

1. **Runtime extension: `@pop:N:@push` ok_dest.** ~10 lines in
   `runtime._dispatch_effects`.

2. **Validate Q2 (name-based identity).** A Python script that
   creates two `Z3_mk_const` with the same symbol+sort and verifies
   they're the same AST. ~30 lines.

3. **Extend the Formula datatype.** Add the 12 new constructors to
   `prelude/z3.ev`. ~30 lines.

4. **Extend materialize.** Add arms for each new constructor.
   Tricky cases: SetUnion (binary chain + malloc array), Forall/
   Exists (bound-variable handling). ~120 lines.

5. **Test programs.**
   - `examples/z3_set.ev`: `x ∈ {1, 2, 3} ∩ {2, 3, 4}` and ask for x.
     Should print x = 2 or x = 3.
   - `examples/z3_quantifier.ev`: `∀ x ∈ {1..3}. x > 0`. Should
     print SAT.

Estimated: ~25 lines of Python (runtime extension), ~150 lines of
Evident (Formula extensions + materialize), ~60 lines of test code.

## What's deferred

- **More general n-ary operations.** N-way `And`, `Or`, larger
  `SetUnion`. v1 uses binary chains.
- **Patterns and weights in quantifiers.** v1 uses simplest form
  (NULL patterns, weight 0).
- **Sort-parameterized FTIs.** `Stack(Seq(Int))` doesn't work in v1
  because `empty(Seq(Int))` isn't expressible. Out of scope here too.

## Open questions for v1

1. **Z3 deduplication of identical ASTs.** Does
   `Z3_mk_int(ctx, 3, sort)` called twice return the same AST?
   If yes, repeated atoms in a Formula tree are free (Z3 caches).
   If no, we pay for each. Probably yes; worth confirming.

2. **The malloc'd intermediate arrays.** Each `SetUnion` allocates
   16 bytes that are never freed. Same v1 leak pattern as Z3 ASTs.
   Document this; future cleanup pass deals with it.

3. **String sort for `Var`.** The `sort_name` field is a String
   like "Int", "Bool", "Set(Int)". For set sorts, we need to
   materialize a set sort first via `Z3_mk_set_sort(ctx, elem_sort)`.
   The materialize function for `Var(name, "Set(Int)")` would need
   to parse the sort name string — annoying. Better: structured
   sort representation.

   Solution: add a `Sort` type:

   ```
   type Sort =
       | IntS | BoolS | RealS | StringS
       | SetS(elem ∈ Sort)
       | SeqS(elem ∈ Sort)
   ```

   And `Var(name ∈ String, sort ∈ Sort)`. This is a Formula-datatype
   change that affects M5 too. Could be done as a follow-up bug fix
   to M5 before M6 starts, or deferred to M6's start. The
   existing `sort_name ∈ String` works for M5 since we only have
   atomic sorts there.

## How to know we're done with this design

When:
- The `@pop:N:@push` ok_dest mechanism is clear and small.
- Q2 (name-based identity) has been validated.
- The materialize arms for each new constructor are spelled out
  enough that an implementation can follow them.

The `Sort` type question may need its own decision before M6 starts.
