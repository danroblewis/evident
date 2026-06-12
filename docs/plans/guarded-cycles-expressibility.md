# Guarded cycles: recursion, type inference, and expressibility

**Type:** design / north-star. No code yet. Not scheduled — captured
now so it isn't lost; sequencing deferred until the self-host work
(record/ordering gaps) settles.

**One-sentence thesis:** recursive value definitions, recursive
*types*, and type *inference* are the same problem wearing three hats —
a dependency cycle that is legal **iff it is guarded** (by a
constructor, a length bound, or a tick), and illegal otherwise (the
"combinational loop"). Build one guardedness check; aim it at three
syntactic sites.

---

## 0. Why these belong together

Evident is, semantically, a **synchronous circuit / coinductive
stream**, not an imperative evaluator (see the prose below for the
full framing). That gives a precise, hardware-grade rule for which
cyclic dependencies are allowed:

- **Within a tick = combinational logic.** A structural cycle here is
  an *infinite inlining* with no finite model for Z3 — forbidden, the
  same way a combinational feedback loop is illegal in hardware.
- **Across ticks = sequential logic.** A cycle is legal, and the thing
  that legalizes it is a **register on the back-edge** — which in
  Evident is the `_x` carry. `A` reads `_b`, `B` reads `_a`: fine, the
  cycle passes through a tick.

The guard can take three forms, one per venue:

| Venue | Cycle | Legal guard | Illegal (unguarded) |
| ----- | ----- | ----------- | ------------------- |
| Values (recursion) | `f` defined via `f` | a length **bound** (finite unfold) **or** a **tick** (`_`) | unbounded + no tick → combinational loop |
| Types (recursive types) | `T` defined via `T` | a **constructor** (`Cons(Int, List)`) | `a = a` → the infinite type |
| Inference (its own fixpoint) | var type depends on var type | the **finite type universe** (CSP terminates) | — (always terminates; the question is 0 / 1 / many solutions) |

So the through-line is one sentence: **a cycle is legal iff guarded by
a constructor, a bound, or a tick.** Everything below is that sentence,
specialized.

Two of the three already work today: semantic value cycles
(`x = y+1 ∧ y = 2·x` — Z3 just solves it) and type cycles (mutually
recursive `enum`s, because Z3 datatypes are built for them). The work
items are the surfaces that *don't* exist yet.

---

## 1. Recursive value definitions (comfortable syntax)

### Goal

Let a programmer write what *looks* like an ordinary recursive
definition and get the correct temporal/bounded lowering — the way
Haskell's surface looks like a normal call but laziness does something
different and load-bearing underneath. **The analogy is exact: a lazy
thunk defers a self-reference until it is observed; an Evident tick
defers a self-reference until the next tick.** "Lazy thunk ↦ next tick"
is the whole lowering.

### Surface (sketch)

A claim/fsm may mention itself:

```evident
claim Sum(xs ∈ Seq(Int), total ∈ Int)
    total = (xs = ⟨⟩ ? 0 : head(xs) + Sum(tail(xs)).total)
```

Reads like FP. Most structural recursion is a **fold** (catamorphism),
so in practice this is "named, possibly-unbounded folds."

### Lowering — the compiler runs the guardedness check on the self-mention

1. **Bounded** (`#xs ≤ N` in scope): the recursion is a *finite*
   unfold → unroll within a single tick into the ternary/∧ chain. This
   is exactly what `scripts/passes/lower-bounded-seq.sh` already does
   for `∀` and folds; the recursive front-door reuses it. No tick axis;
   the fixed point is a bounded value.
2. **Unbounded but tick-guarded** (the recursive result is read as
   `_total` next tick): lower to the trampolined cursor-walk (the FSM).
   The recursive call *becomes* "advance the cursor; read the
   accumulator next tick." This is the C2Items/C2H trampoline pattern
   that `compiler2/driver.ev` already is, by hand.
3. **Unguarded** (unbounded + no tick on the back-edge): the
   combinational loop — reject with an error that writes itself:
   *"unguarded self-reference; bound the sequence or thread the result
   through a tick."*

### Design questions to resolve before building

- **Where the accumulator lives** in the unbounded case (carried state
  threaded by the trampoline) and how its type/shape is declared.
- Whether the surface is a self-mention (above) or a dedicated
  `fold`/`rec` keyword. Self-mention is more FP-like but needs the
  recognizer to distinguish it from ordinary composition.
- The boundedness analysis that picks unroll-vs-trampoline must be the
  *same* analysis the strict hoist (below) uses to detect combinational
  loops — do not build two.

### Relationship to the bounded↔unbounded fixed-point boundary

Recursion ultimately computes a **fixed point**. Z3 finds bounded
fixed points *directly* (no iteration). You only spill onto the tick
axis when the fixed point is an *unbounded structure* (a list of
arbitrary length, a tree of arbitrary depth) rather than a value. So
the unroll-vs-trampoline decision is literally "is the fixed point a
bounded value or an unbounded structure." A future nicety: have the
compiler *detect and report* which side a definition lands on, instead
of the author discovering it via a slow solve.

---

## 2. Type inference as a finite-domain CSP

### Goal

Delete the ubiquitous declaration line. Today a huge amount of code is
pairs like:

```evident
read_sort ∈ Effect
BuildMemReadLong(addr ↦ sortsout_p + (8 * _read_cur), eff ↦ read_sort)
```

The `∈ Effect` is recoverable: `read_sort` is bound to the `eff` slot
of `BuildMemReadLong`, whose header declares `eff ∈ Effect`. Drop the
decl; infer it. Across the codebase this is a large line-count win, and
line count is a first-class goal (see §4).

### Why Evident's inference is special

Evident's type universe is **finite and closed per program**: the
builtin scalars, `Effect`/`Result`, the program's own `type`/`enum`
decls, and explicit generics. There is no open HM-style polymorphism to
chase. So inference is **not** unification-to-a-most-general-type; it is
a **finite-domain constraint satisfaction problem**:

- each untyped variable gets a domain (its candidate types);
- each usage emits a constraint (`eff ↦ read_sort` ⇒ `read_sort :
  Effect`; `8 * _read_cur` ⇒ `_read_cur : Int`);
- solve for a consistent assignment.

That is a SAT/CSP — **Z3's home turf.** The inference pass could *be a
`.ev` program solved by the same Z3 the runtime ships.* This is the
self-hosting story extended to the type layer, and it is uniquely
natural for Evident among languages.

### The two design decisions the constraint framing hands you

1. **Determinism = uniqueness, not generality.** There is no "most
   general" type in a finite universe — there is the *set of satisfying
   assignments*. The honest rule: **infer a type iff the CSP has exactly
   one solution.** Zero → a real type error (conflicting usages). Two or
   more → *ambiguous*; require an annotation. Both conditions are
   decidable and make crisp errors.
2. **The occurs-check is the guardedness check (again).** Cyclic type
   constraints are fine iff the cycle passes through a constructor
   (`Cons(Int, List)`); an unguarded type cycle (`a = a`) is the
   infinite type — the type-level combinational loop. Reuse §0's rule.

### The risk, and why the project's philosophy defuses it

Global inference has a known hazard: **diffuse blame.** When a
variable's type comes from usages scattered across the program, an
UNSAT/ambiguity has no obvious *place*; a usage added in one corner can
retype or break something far away. This is why Haskell limits
let-generalization and Rust demands annotations at `fn` boundaries —
they trade inference power for *local* error locality.

Evident makes the opposite bet **on purpose**, and the bet is
self-consistent: a variable first appearing at a use-site (with no
declaration) forcing you to read the whole program is **intended** —
the unit of reasoning is the *whole program*, not the local scope,
because programs are kept small enough to hold in one head. Diffuse
blame is only confusing when the program is bigger than your head.
**The two choices are load-bearing for each other:** whole-program
inference is only sane in small programs, and inference is one of the
things that keeps programs small. Remove either and the other degrades.
This is a coherent design optimizing for *global comprehension over
local reasoning* — the inverse of the mainstream locality bet.

### Consequence for the build

The inference engine's most important output is **not** the inferred
types — it is the **ambiguity/conflict report**. *"`read_sort` is
constrained to `Effect` by line 412 and to `Int` by line 880"* is the
artifact that replaces the declaration you deleted. Get that report
excellent and the feature is a joy; get it wrong and it is exactly the
frustration the feature risks. (Prior inference attempt: see git
history; the new framing is the finite-domain-CSP + uniqueness criterion
above, which the earlier attempt did not have.)

---

## 3. Prior art / cross-references in this repo

- **The trampoline already exists, by hand:** `compiler2/driver.ev`'s
  `C2Items` work-list + `C2H` handle stack + `C2Frames` *is* a
  recursive-descent translator defunctionalized into a tick-walked
  stack. Recursion §1 is "let people write that without hand-rolling
  the trampoline."
- **Bounded unroll already exists:** `lower-bounded-seq.sh` (the
  `∀`/`∃`/fold → covered-ternary-chain lowering). Recursion §1's bounded
  case is a front-door onto it.
- **The productivity check already exists, operationally:** the kernel's
  "stuck" halt (`state_next == state` with no `Exit` → exit 1, per
  CLAUDE.md halt conditions) is a runtime **coinductive
  well-formedness** check — it catches a stream that stopped emitting
  anything new. A *static* version of this is a possible future item
  (totality/productivity analysis).
- **Carry-threading composition** (`docs/plans/fsm-composition.md`) is
  the temporal-cycle mechanism §0 leans on.

---

## 4. Related expressibility items (mentioned, not yet specced)

Same north star — **shrink programs until a person can read one
all-at-once** — different levers. Captured here so they live somewhere;
each needs its own doc.

- **Alternatives to ternary chains.** The `a ? b : c` chain is the
  dominant shape and a readability tax (see the user-memory note
  "no ternary chains in source"). Want: a set-theoretic / pattern pin
  surface that the transform layer assembles into the covered chain the
  functionizer needs — keep the surface, change the lowering. Candidate
  surfaces: guarded set-membership pins, table/case forms,
  match-as-expression sugar.
- **Set-theoretic runtime redesign.** Refactor the runtime/stdlib
  toward a more set-theoretic design (membership, comprehension, image)
  rather than index/cursor-heavy code — aligns with "a schema is a named
  set defined by membership conditions" and with the registry-by-key
  (not by-position) idiom.

---

## 5. Sequencing

Not scheduled. The honest ordering when it *is* time:

1. **The strict guardedness checker** — the shared dependency of
   everything here (and an immediate upgrade to the declaration-hoist
   pass: from "hoist decls" to "detect a same-tick structural cycle and
   reject it"). Build this first; it is the one reusable piece.
2. **Type inference** — highest line-count payoff, self-contained
   (a CSP pass + a blame report), and dogfoods the constraint engine.
3. **Recursive-definition surface** — depends on (1) for the guard
   decision and reuses `lower-bounded-seq` for the bounded case.
4. Ternary-alternative surface and set-theoretic runtime — independent
   tracks, own docs.
