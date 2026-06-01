# FTI composition — how `s ∈ Stack(Int)` actually works

This document is the design-before-implementation pass for FTIs. The
prelude plan (`prelude-plan.md`) says the composing FSM declares
`s ∈ Stack(Int)` to bring the Stack FTI's variables into scope, and
asserts relations like `s.contents = _s.contents ++ [42]` to push.
What that actually means — at the parser, transpiler, and runtime
level — wasn't pinned down. This doc pins it down.

The agent that landed M1/M2/M7 explicitly flagged this gap. M3
shouldn't start until the questions here are resolved.

## The four sub-problems

The composition mechanism breaks into four concrete questions:

1. **Parsing.** What does `s.contents` and `_s.contents` look like at
   the token level?
2. **Type expressions.** How does `Stack(Int)` work as the right-hand
   side of `s ∈ Stack(Int)`? It's not a builtin sort like `Int`.
3. **Transpiler.** When the FSM body has `s ∈ Stack(Int)`, how does
   the transpiler bring in the Stack FTI's variables and constraints?
4. **FTI body semantics.** How does the FTI's body relate the state
   pair `(_x, x)` to the `effects` channel? What forms of transition
   does it support?

Each gets its own section below.

## 1. Parsing: dotted names

The user writes `s.contents`. The parser must recognize this as a
*qualified identifier*: a name in the namespace `s`.

**Proposed grammar:**

```
qualified_ident ::= IDENT ("." IDENT)+
```

The bootstrap parser already tokenizes `.` (it appears in `..` for
ranges). For dotted names, `.` between two IDENTs should produce a
qualified-name token sequence, parsed as a single AST node:

```
{"kind": "qualified", "parts": ["s", "contents"]}
```

The leading-underscore case `_s.contents` is *not* a parse of
`_(s.contents)`. It's a qualified name where the *first part starts
with underscore*: `_s.contents` parses to `{"kind": "qualified",
"parts": ["_s", "contents"]}`. The underscore is part of the IDENT
`_s`, which mirrors the regular state-pair convention.

**At the transpiler level**, a qualified name lowers to a flat
SMT-LIB identifier with the parts joined: `s.contents` →
`s_dot_contents` or simply `s.contents` (SMT-LIB allows dots in
quoted identifiers, but flat is safer). I'd go with a flat scheme
like `s__contents` (double underscore) — unambiguous, easy to grep,
no quoting needed.

This is bug-fix-shaped: ~15 lines in the lexer (recognize dotted
sequences as qualified-name tokens) and the parser (treat them as a
new AST kind), plus ~5 lines in the transpiler (emit flat form).

## 2. Type expressions: `Stack(Int)` as a set-expression

The current `set_expr` grammar handles three forms:

```
set_expr ::= IDENT                        ; "Int", "Bool", or a named type
           | IDENT "(" set_expr ")"       ; "Seq(Int)", a generic builtin
           | "{" lo ".." hi "}"           ; range
           | "{" e ("," e)* "}"           ; enumeration
```

`Stack(Int)` parses as the second form (generic application). The
parser produces:

```
{"kind": "set_named", "name": "Stack", "param": {"kind": "set_named", "name": "Int", "param": None}}
```

What changes is what the transpiler does with it.

**For builtin sorts** (`Int`, `Bool`, `Seq(Int)`), the transpiler
emits SMT-LIB sort references directly — `Int`, `(Seq Int)`.

**For FTI types** (`Stack(Int)`, `Queue(String)`, `Z3`), the
transpiler must instead:

1. Look up the FTI declaration by name.
2. Substitute the type parameter (e.g., `T = Int`).
3. Inline the FTI's variable declarations under the namespace of the
   variable being declared.
4. Inline the FTI's constraints under the same namespace.

How does the transpiler distinguish a builtin sort from an FTI? By
looking up the name in an **FTI registry**. The registry is populated
from `prelude/*.ev` files at compile time. `Stack`, `Queue`, `Z3`,
etc., live in the registry; `Int`, `Bool`, `Seq`, `Real`, `String` do
not.

This means the transpiler must, at compile time, parse all FTI
declarations in `prelude/` *before* transpiling the user program.
First pass: load FTI definitions. Second pass: transpile the user
program, expanding FTI references inline.

## 3. Transpiler: inlining an FTI

Given a user program that declares `s ∈ Stack(Int)` inside an `fsm`
body, and an FTI declaration:

```
fti Stack(T)
    base ∈ Int
    contents ∈ Seq(T)
    effects ∈ Seq(Effect)
    ; ... body constraints ...
```

The transpiler must emit, in the composed body:

```smt2
; State pairs for s's variables (namespaced)
(declare-const _s__base Int) (declare-const s__base Int)
(declare-const _s__contents (Seq Int)) (declare-const s__contents (Seq Int))

; The FTI's effects channel, also namespaced
(declare-const _s__effects (Seq Effect)) (declare-const s__effects (Seq Effect))

; The FTI's body constraints, with `T` substituted and every variable
; renamed to its namespaced form
(assert ... namespaced constraint ...)
```

Then `s.contents` in the user's FSM body lowers to `s__contents`, and
`_s.contents` lowers to `_s__contents`. The user writes:

```
s.contents = _s.contents ++ [42]
```

The transpiler emits:

```smt2
(assert (= s__contents (seq.++ _s__contents (seq.unit 42))))
```

This constraint joins the FTI's own internal constraints in the same
combined body. Z3 sees one big system and solves it.

**Effects channels.** Each FTI has its own `effects` channel
(namespaced: `s__effects`). The runtime's effect dispatcher already
scans for `effects` and `*_effects` consts — the namespaced channel
gets picked up automatically. The runtime fires the libcalls without
needing to know about FTIs at all.

**Multiple FTIs.** A user FSM can declare multiple FTI variables;
each gets its own namespace. Their effects channels are separate
(`s__effects`, `q__effects`) and dispatched independently.

## 4. FTI body semantics

This is the hardest part and what the user pushed back on.

The FTI's body is constraints over its state-pair variables plus its
effects channel. There is no separate "command" mechanism. The body
relates the state pair to the effects directly.

**What the FTI body looks like** for Stack:

```
fti Stack(T)
    base ∈ Int
    contents ∈ Seq(T)
    effects ∈ Seq(Effect)

    ; On init: allocate external storage; _contents starts empty.
    is_init ⇒ effects = [
        LibCall("__mem__", "alloc", "l(l)",
                [ArgInt(8192)], "base", "")]

    ; Steady state: the relationship between _contents and contents
    ; determines what libcalls fire.
    ¬is_init ⇒ effects = match (len(contents) - len(_contents)):
        1  =>
            ; Pushed one element. The new last is `last(contents)`.
            ; Write it at the appropriate external offset.
            [LibCall("__mem__", "store_long", "v(ll)",
                     [ArgInt(base + len(_contents) * 8),
                      ArgInt(last(contents))],
                     "", "")]
        -1 =>
            ; Popped one element. External storage doesn't need
            ; rewriting; we just don't read those bytes anymore.
            []
        0  =>
            ; No change.
            []
        _  =>
            ; Unsupported transition shape.
            ; This case is what we have to decide about.
            ???
```

**What the FTI body does NOT have:**

- No `cmd ∈ StackCmd` port. The composing FSM does not "send commands."
- No method-call syntax. There is no `s.push(42)`.
- No imperative phase machine inside the FTI. The body is constraints.

**The composing FSM writes:**

```
s.contents = _s.contents ++ [42]    ; push
s.contents = init(_s.contents)      ; pop
s.contents = _s.contents            ; no-op
```

Each of these is one constraint in the combined body. The FTI's match
on length-delta picks the right libcalls. Z3 solves; the runtime
dispatches.

## The unbounded-data tradeoff

Here's the honest tension:

- The FTI's `contents` is a Z3 `Seq` that, at any tick, holds the
  full current stack contents.
- Each tick, `_contents` is pinned to the prior tick's `contents`.
- For a stack of depth 100, this means a 100-element Seq is pinned
  as input on every tick's solve.

This is exactly the "growing data in the body" pattern CLAUDE.md
calls out as a failure mode. We're hitting it knowingly.

Three options:

**Option A: Accept the cost for v1.** Z3's sequence theory can
handle Seqs symbolically; small-to-medium stack depths should be
fast enough. The user is warned in documentation that Stack depth
affects per-tick solve time.

**Option B: Window the FTI's view.** The FTI exposes only the top K
elements (e.g., top 4) as Z3 variables. The rest of the stack lives
only in external memory. The composing FSM can `push`, `pop`, and
read the top few — but can't reason about deeper elements via Z3
relations. This is much more bounded but loses expressiveness.

**Option C: Don't expose `contents` as a Seq at all.** Expose only
`top` and `size`. Composing FSM writes constraints like `top = 42,
size = _size + 1` to push, and `size = _size - 1` to pop. The FTI
detects from the `(size, _size)` pair what to do. This is cleaner
but loses the "write `++ [42]`" relational nicety.

**Recommendation: Option A for v1.** It's the most honest
implementation and matches the user's stated relational preference.
We accept the perf cost for now; if Stack depth becomes a perf
problem in practice, we revisit with B or C as alternatives.

## The "unsupported transition" question

This is the question the user pushed back on. What happens if the
composing FSM writes something the FTI's case analysis doesn't
handle?

Example: `s.contents = reverse(_s.contents)`.

Three possible answers:

**Answer 1: Silently solve, no libcalls fire.** The Z3-side
constraint is satisfiable (Z3 will pick a value for `contents` that
matches the reverse), but the FTI's match has no case for this
transition shape, so `effects = []` and external memory diverges
from the model. **Wrong.** The model lies about what's in external
storage.

**Answer 2: Force UNSAT on unsupported transitions.** The FTI's
body asserts that contents must be one of the supported shapes:

```
contents = _contents
∨ ∃ x. contents = _contents ++ unit(x)
∨ contents = init(_contents)
```

If the user writes something else, Z3 returns UNSAT, the runtime
halts with an error. **Honest.** The composing FSM is told its
constraint is incompatible with this FTI's transition set.

**Answer 3: Generic materialization fallback.** When the case
analysis doesn't match, emit a libcall sequence that wholesale-
rewrites external memory to match the new contents. **Hard** — the
sequence length depends on contents' length, which is unbounded.
Probably requires multi-tick rewriting and breaks the "one tick =
one logical step" property.

**Recommendation: Answer 2.** The FTI honestly declares which
transitions it supports. If the user wants other transitions, they
write a different FTI. The constraint approach gives us this for
free — we just have to assert it.

For the Stack FTI in v1, the supported transitions are: no-op,
push (any value), pop. That's it. Anything else is an error and the
program halts.

This is honest, type-safe (in the SMT sense), and forces the user to
write FTIs that match their intended use rather than expecting the
implementation to handle arbitrary relations.

## What needs to land for M3

Concretely, in order:

1. **Parser bug fix: dotted names.** Tokenize `s.contents` as a
   qualified name; emit a `qualified` AST node. ~15 lines.

2. **Transpiler: FTI registry and inlining.** Parse `prelude/*.ev`
   FTI declarations at compile time; build a registry; when a user
   FSM body has `x ∈ FTIType(params)`, inline the FTI's vars and
   constraints with namespace mangling. ~80 lines.

3. **The Stack FTI itself.** Written in Evident as `prelude/stack.ev`.
   The body uses the supported-transitions-only pattern from Answer
   2. ~50 lines.

4. **A small test program.** Push 1, 2, 3; pop them; verify the
   external memory was written and that pop returned the right
   values. (The "popped value" comes from reading the cell that was
   the previous top — separate libcall logic.)

5. **The "unsupported transition" test.** A program that asserts a
   reverse-like transition; verify it halts with UNSAT cleanly.

Estimated work: ~50 lines of Python (parser + transpiler), ~80
lines of Evident (prelude/stack.ev + tests), plus the design test
that validates UNSAT behavior.

## Remaining open questions

1. **How does the user *read* a popped value?** If the user writes
   `s.contents = init(_s.contents)`, the popped element is
   `last(_s.contents)`. The user can name it: `popped =
   last(_s.contents)`. This works because `_s.contents` is in scope
   and `last` is a built-in seq idiom from M7. But the user has to
   remember to capture it before the pop "commits" — meaning before
   the next tick when `_s.contents` becomes the post-pop value.
   Convention to document: capture popped values in the same tick
   you pop.

2. **How does init handle pre-existing external state?** If external
   storage was already populated by a prior run, what does
   `_contents` look like on tick 0? Probably: empty (we treat each
   program run as starting fresh). Document this.

3. **Multiple instances of the same FTI.** `s ∈ Stack(Int)` and
   `t ∈ Stack(Int)` in the same FSM. Each gets its own namespace
   (`s__base`, `t__base`) and own external allocation. The init
   libcalls fire for both independently.

4. **Parameter types other than `Int`.** `Stack(String)` would
   require the libcalls to store/load String values, which uses
   different ctypes than `c_long`. The `__mem__` ops we have support
   only longs. Either extend `__mem__` with string ops, or restrict
   v1 to int-typed FTIs. **Recommend: int-typed only for v1.** Add
   string support as a separate bug fix when needed.

5. **The `init` constraint depends on `is_init`, which is in the
   composing FSM's scope.** When inlining the FTI, `is_init` is the
   shared global `is_init` declared in the prelude. This works
   because the composing FSM and the inlined FTI body share the same
   `is_init` const — they're solved together each tick. Confirm this
   in the transpiler implementation.

## What's not in this doc

- M5 (Z3 FTI). The Formula datatype plus Z3 FTI is its own design
  question that builds on what's resolved here but has additional
  complexity (Formula tree marshaling, the two-tick latency for sat
  results, etc.). Defer to its own design doc when M3 is done.
- M4 (Queue FTI). Should be structurally identical to Stack with
  different supported-transitions (enqueue at tail, dequeue from
  head). Once M3 lands, M4 is mostly mechanical.
- Out-parameter sigs (for libcall functions that return values into
  buffer pointers). Not needed for Stack with `__mem__`; will come
  up later.

## How to know we're done with this design

When the following make sense and are coherent:

- The parser bug fix (dotted names) is small and well-scoped.
- The transpiler's FTI registry + inlining mechanism has a clear
  data flow: read all `prelude/*.ev` → build registry → transpile
  user programs by inlining.
- The Stack FTI body, as sketched above, is exactly the SMT-LIB the
  transpiler will produce after substitution.
- The supported-transition assertion (Answer 2) gives a clean error
  on bad transitions.

If all four are clear, M3 is ready to implement. If any of them are
still hand-wavy, that's the next design pass.
