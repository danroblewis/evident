# Phase 3.1: Recursive claim invocation

## Goal

Let a claim call itself. Required for any AST walker — the body of
a Recursive datatype is itself the same Recursive datatype.

## Today's limitation

`commands/desugar.rs` shows the workaround: iterate flat indices
in Rust, calling Z3 once per index. This works for flat seqs but
not for tree-shaped data (an Expr's body is itself an Expr). To
walk a tree in Evident, the claim needs to recurse.

## Design sketch

Z3 supports recursive functions via `(define-fun-rec ...)` and
recursive Datatypes via mutually-recursive `create_datatypes`.
Evident's claim system today is non-recursive: a claim is inlined
into its caller's body once. To recurse, we need:

1. **Identify recursion** at load time: a claim that names itself
   in its body.
2. **Compile to Z3 recursive function** instead of inlining.
3. **Bound the recursion** somehow — Z3 can fail to terminate on
   unbounded recursion. Options:
   a. User specifies a depth bound: `RecursiveClaim<10>`.
   b. Detect a structurally-decreasing arg (Cons → tail) and assert
      well-foundedness implicitly.
   c. Emit Z3 recursive functions and let Z3 handle it (it has
      built-in support but not always termination guarantees).

Recommend (b) for v1: detect Cons → tail as the recursive descent
pattern.

## Files touched

- `runtime/src/translate/inline.rs` — split inline path: if the
  claim recurses, emit a recursive Z3 function instead of inlining.
- `runtime/src/translate/datatypes.rs` — possibly extended for
  the new recursive-function machinery.
- New tests for recursive walks.

## Test it

A program that sums the elements of a LinkedList:

```evident
enum LinkedList = LLNil | LLCons(Int, LinkedList)

claim sum(list ∈ LinkedList, total ∈ Int)
    list = LLNil ⇒ total = 0
    ∀ h ∈ Int, t ∈ LinkedList :
        list = LLCons(h, t) ⇒
            ∃ rest ∈ Int :
                sum(t, rest)        -- self-call
                total = h + rest

claim t
    list ∈ LinkedList = LLCons(1, LLCons(2, LLCons(3, LLNil)))
    total ∈ Int
    sum(list, total)
    total = 6
```

This should be SAT.

## Acceptance

- [ ] LinkedList sum example works
- [ ] Tree-walking example (sum nodes of a recursive tree) works
- [ ] Termination on bounded structures
- [ ] LOC: +~400 Rust (translator path), 0 Evident
- [ ] Existing tests still pass

## Notes

This is a real language feature with real design decisions.
Significant engineering. Allocate accordingly.

The most complex bits will be:
- Detecting recursion in a claim body
- Mapping Evident claim args to Z3 recursive function args
- Handling the inductive proof of termination

If Z3's `define-fun-rec` doesn't get us where we need, fall back to
bounded unrolling (k iterations, then UNSAT for deeper structures).
A bound is acceptable for v1.
