# The minimal-runtime implementor contract

> **The one question this doc answers:** *what must someone write to
> implement an Evident runtime?* Not "what does today's runtime contain"
> (that is [`minimal-runtime.md`](minimal-runtime.md)) and not "what can
> be moved to Evident" (that is
> [`self-hosting-inventory.md`](self-hosting-inventory.md)). This is the
> **conformance contract**: the irreducible kernel an implementor is
> obligated to write, and the principle that everything else is either
> shipped *in Evident* (stdlib, written once) or is an *optional
> host-language accelerator* that, if absent, falls back to the kernel.
>
> It is the project's north-star answer to "how small can a runtime be?"
> A conformant Evident runtime is *this small*; the rest is libraries and
> optional speed.
>
> Companion reading, in the order it grounds the argument:
> [`minimal-runtime.md`](minimal-runtime.md) (the ~11K target + FFI-first
> vision), [`self-hosting-inventory.md`](self-hosting-inventory.md) (the
> tier ladder — every `runtime/src` file classified), [`../self-hosting.md`](../self-hosting.md)
> (the swap-interface seam), and the strategy docs that *prove* the
> two-bucket principle:
> [`selection-policy.md`](selection-policy.md),
> [`nested-fsm-strategies.md`](nested-fsm-strategies.md),
> [`loop-functionizer.md`](loop-functionizer.md),
> [`cegar-scaffolding.md`](cegar-scaffolding.md).

---

## § 1 — The two-bucket principle

Minimizing implementor burden is the same problem as defining the
irreducible **kernel**. Everything a runtime does is one of three things,
and only the first is the implementor's job:

> **(kernel)** The irreducible core — front end, the constraint interface
> to an SMT solver, effect dispatch to the OS, and the scheduler that
> runs FSMs (including over recursive enums). An implementor **must**
> write this. It is the seed nothing else can be expressed without.
>
> **(a) self-hosted stdlib** — the AST→AST and AST→X transforms (validate,
> subscriptions, pretty, and eventually desugar / generics / inject),
> written *once* in Evident and shipped with the language. They run *on*
> the kernel. An implementor gets them **free** — re-writing them in the
> host language is forbidden, not merely discouraged (§ 3, § 4).
>
> **(b) optional accelerators** — every functionizer (Cranelift, GLSL,
> LLM, Symbolic, Satisfier) and every fast nested-FSM tier
> (symbolic-unroll→JIT, loop-functionizer). Each is a *speed* layer over
> an always-correct kernel slow path. An implementor **may** write one for
> performance, or **skip** it and fall back to the kernel.

So: an implementor writes the kernel. They get (a) for free. They *may*
write (b) for speed, or skip it. That is the whole contract.

### The functionizer/tier ladder is the proof

This is not an aspiration — it is the shape the codebase already has, at
two levels, both built as *try-the-fast-path, fall-through-to-an-always-
correct-floor*:

**Level 1 — the single-solve functionizer fall-through.** In
`runtime/src/runtime/query.rs`, `try_functionize_z3` attempts to compile
a claim's components to a `CompiledFunction`; on refusal (`compile →
None`, or a `call → None` at runtime) the runtime falls through to
`evaluate(...)` — the full Z3 solve. The five functionizers
(`functionize/{cranelift,symbolic,llm,glsl,satisfier}.rs`) are
interchangeable accelerators; the Z3 solve is the floor. Strip all five
→ correct, slow runtime.

**Level 2 — the nested-FSM strategy selector**
([`nested-fsm-strategies.md`](nested-fsm-strategies.md) § 3). `run(F,
init)` tries tier 1 (symbolic-unroll → JIT), falls to tier 2
(loop-functionizer), falls to tier 3 (**blocking-interpret** — run `F` on
the existing multi-FSM scheduler). Tiers 1 and 2 compile; tier 3 compiles
nothing and reuses the scheduler verbatim, so it is **always correct** and
serves as the equivalence oracle the faster two are validated against.
Strip tiers 1 and 2 → correct, slow nested runs.

Both levels mirror each other exactly — [`selection-policy.md`](selection-policy.md)
§ 1 names this as the deliberate design (its selection-policy axis lives
*inside* `query.rs`'s fall-through; the nested selector is "the sibling
chooser one level up"). [`cegar-scaffolding.md`](cegar-scaffolding.md)
§ 1 names the general form: a `CompiledFunction` is an *abstraction*; the
full Z3 solve is the *oracle of truth*; refusal escalates to the oracle.

**The conclusion this forces:** every accelerator the project has built —
all five functionizers, both fast nested-FSM tiers — was correct to build
*on top of* a slow path that shipped first. "Build the slow path first"
has been right at every step because the slow path is the kernel and the
fast path is bucket (b). The accelerators can be deleted, one at a time,
and the runtime stays correct — only slower. That property *is* the
two-bucket principle, stated operationally.

> **One caveat the accelerator framing must respect** (it sharpens § 4):
> for the **recursive tree-walk** class, the Z3 solve is **not** a sound
> oracle — a solve over an unbounded recursion *is* the recursion gap
> (`examples/COUNTEREXAMPLES.md` #15; [`loop-functionizer.md`](loop-functionizer.md)
> § 6). There, the always-correct floor is **not** Z3 — it is the
> *scheduler running the walk to a concrete result*. That is why
> tree-walking lands in the kernel and not in bucket (b): its slow path is
> a scheduler capability, not a solver call.

---

## § 2 — The irreducible kernel (what an implementor MUST write)

Four pieces. For each: what it is, *why it is irreducible* (cannot be
self-hosted, cannot be skipped), and a rough LOC budget anchored to
[`minimal-runtime.md`](minimal-runtime.md)'s "what stays in Rust" table.

### 1. Front end — lexer + parser + AST (text → AST)

**What.** Read source bytes, produce the `Program` AST
(`core/ast.rs`'s `Expr` / `BodyItem` / `SchemaDecl` / `Effect`).

**Why irreducible.** A seed is unavoidable. Parsing Evident *with* Evident
is circular by construction: you need an AST before you can run any pass,
and you need a parser to get an AST. The AST node definitions are the
shared vocabulary the rest of the kernel and every stdlib pass agree on
([`self-hosting-inventory.md`](self-hosting-inventory.md) marks all of
`parser/`, `lexer.rs`, and `core/ast.rs` Tier 0 "forever"). The one
escape — a *minimal* seed parser that loads `stdlib/parser.ev`, which then
parses the full language — is a real future option (§ 6), but a seed of
*some* size is irreducible.

**Budget.** Lexer ~400, parser ~1,900, AST ~350 → **~2,650 LOC**. The
parser is the largest single irreducible file.

### 2. Constraint interface — primitive lowering + solver FFI + model marshaling

**What.** Three sub-parts that together *are* the language's semantics:
(i) lower the *primitive* constraint forms — equality, arithmetic,
quantifier unrolling over pinned lengths, set/seq membership, enum
construction/recognition — to solver assertions; (ii) the FFI binding to
the SMT solver (Z3) that declares typed consts and checks satisfiability;
(iii) marshal a satisfying model back to `Value`.

**Why irreducible.** This is an **FFI boundary** to a C library (Z3), and
it is the runtime's reason to exist. Self-hosting it would require
self-hosting Z3 — a different project entirely
([`minimal-runtime.md`](minimal-runtime.md), "what we don't try to
remove"). Note the scope word: *primitive*. Much of today's `translate/`
(~5,200–7,500 LOC) is **desugaring** stacked on top of the primitive
lowering — Seq-concat flattening, record-lift, match→ITE, prefix
injection — and the inventory flags those parts Tier 1/2 (self-hostable
AST→AST; `translate/{preprocess, exprs/record_lift, exprs/match_expr,
inline/rewrite, inline/dispatch}`). The *irreducible* core is the part
that calls the Z3 API and decodes its models (`translate/{declare,
extract, eval/*}`, `z3_eval.rs`, the `core/z3_*` data types). The exact
line between "irreducible lowering" and "self-hostable desugar" is the
sharpest open question (§ 6).

**Budget.** The Z3-API-calling core (declare + assert primitives + eval +
extract + `z3_eval` + `core/z3_*`) is the bulk of the runtime: **~5,000
LOC** once the self-hostable desugaring is subtracted from today's
`translate/`. (Today's full `translate/` measures larger because the
desugar passes still live in Rust.)

### 3. Effect dispatch — FFI to the OS for the exposed effects

**What.** Walk an FSM's emitted effect list and perform each: a few
built-ins that pre-date FFI (Print, Read, Time, Exit) and a generic FFI
primitive (`dlopen` / `dlsym` / libffi call + type marshaling) through
which everything else reaches C.

**Why irreducible.** Syscalls. A constraint solver cannot print, read a
socket, or call into libSDL — those cross the boundary out of the
constraint world ([`self-hosting-inventory.md`](self-hosting-inventory.md)
Tier 4 "unbounded / external"). The architectural bet
([`minimal-runtime.md`](minimal-runtime.md)) is that this surface is
*small and generic*: one effect dispatcher + one FFI primitive, over which
all domain effects (graphics, audio, network) are **Evident libraries**,
not Rust plugins. The kernel knows about effects; it does not know about
SDL.

**Budget.** Built-in effects ~200, FFI primitive ~700, plus the IO
dispatch surface (`effect_dispatch.rs`, today ~1,100) → **~900–2,000 LOC**
depending on how many built-ins survive as FFI wrappers (§ 6).

### 4. Scheduler / FSM execution — including the recursive-enum tree-walk

**What.** The multi-FSM tick loop: each tick solves `(state_next, effects)
given (state, last_results)`, performs the effects, feeds results to the
next tick, halts when no FSM is scheduled or one emits `Effect::Exit`
(`effect_loop/`, `runtime/scheduler_api.rs`). **And, load-bearingly:** the
ability to **run an FSM-with-stack over any recursive enum to completion
with composite state** — the general tree-walk capability (§ 4).

**Why irreducible.** The step loop is the execution model — there is no
program without it. The tree-walk capability is irreducible for a subtler
reason that § 4 develops in full: it is what lets the *self-hosted stdlib
passes* run on the kernel at all. Without it, an implementor cannot run
`validate` / `subscriptions` / `pretty`, would re-implement them in the
host, and self-hosting collapses — *raising* implementor burden. So it
belongs in the required kernel, not behind an optional accelerator.

**Budget.** Step engine ~300, plus state encode/decode and the
nested-run adapter (`effect_loop/{state, nested}.rs`) → **~500–700 LOC**.
The scheduler *loop* shell (`effect_loop/{mod, scheduler}.rs`, Tier 4) is
larger but much of it is IO orchestration; the irreducible *execution*
core is small.

### Total, and the relation to ~11K

| Kernel piece | Irreducible LOC (est.) |
|---|---|
| 1. Front end (lexer + parser + AST) | ~2,650 |
| 2. Constraint interface (primitive lowering + Z3 FFI + marshal) | ~5,000 |
| 3. Effect dispatch (built-ins + FFI primitive + IO surface) | ~1,500 |
| 4. Scheduler / FSM execution + recursive-enum tree-walk | ~700 |
| Runtime API + CLI shell (glue) | ~1,000 |
| **Kernel total** | **~10,850** |

That lands just under [`minimal-runtime.md`](minimal-runtime.md)'s
~10,050–10,250 "core" estimate and the ≤11K
[`../plans/README.md`](../plans/README.md) target — *not by coincidence*.
The ~11K target **is** the kernel. The gap between today's ~17K–35K
`runtime/src` and that floor is exactly the two buckets that should not be
there: self-hostable passes still living in Rust (bucket a, ~4,400 LOC of
Tier 1/2), and accelerators that an implementor needn't write (bucket b).
The floor moves lower still if the parser becomes a self-hosted pass over
a seed (~6,350, § 6) or if the `translate/` desugaring half migrates to
stdlib (~8,250).

---

## § 3 — What is NOT in the kernel

Two categories, mapped to [`self-hosting-inventory.md`](self-hosting-inventory.md)'s
tiers. **An implementor writes neither.**

### Self-hosted stdlib passes — written in Evident, run on the kernel

The AST→AST and AST→X transforms: `validate`, `subscriptions`, `pretty`
today; `desugar`, `generics`, `inject` next (the inventory's Tier 1/2,
~4,400 LOC of Rust that should leave). These are shipped *with the
language* as `stdlib/passes/*.ev`, loaded and run by the kernel through
`EvidentRuntime::query` ([`../self-hosting.md`](../self-hosting.md)). An
implementor who has written the § 2 kernel can run them as-is. **They do
not rewrite these** — that is the entire point of self-hosting, and the
LOC payoff of the ≤11K target (every pass that moves to Evident is Rust
the implementor never writes and the project maintains once).

Today the seam keeps a Rust impl as the *constructible default*
([`../self-hosting.md`](../self-hosting.md), "selection by construction")
— but that is a migration scaffold, not the contract. The contract is:
these passes are libraries, in bucket (a).

### Accelerators — optional speed over the kernel slow path

Everything in the proof of § 1: the five functionizers
(`functionize/*.rs`) and the fast nested-FSM tiers (symbolic-unroll → JIT,
loop-functionizer). Each falls back to the kernel when absent or when it
refuses a shape. An implementor **may** write a Cranelift JIT to make
steps run at ~µs instead of a Z3 solve per tick; they **may** write the
loop-functionizer to drain a work-stack in native code instead of via
scheduler ticks. Skipping all of them yields a correct, slow runtime.

### The reconciliation: inventory Tier 0 ⊋ this doc's kernel

A reader coming from [`self-hosting-inventory.md`](self-hosting-inventory.md)
will notice an apparent contradiction: the inventory marks
`functionize/*.rs` **Tier 0 — "Kernel"**, yet § 1 here calls
functionizers *accelerators* (bucket b, optional). Both are right; they
answer **orthogonal questions**:

| Axis | Question | Tiers |
|---|---|---|
| **Self-hostable?** (inventory) | Can this be written in Evident and run on the kernel? | Tier 0 = no (circular/bootstrap) · Tier 1/2 = yes · Tier 4 = no (IO) |
| **Required for conformance?** (this doc) | Must an implementor write this for a *correct* runtime? | kernel = yes · bucket a/b = no |

The inventory's Tier 0 ("not self-hostable") is a **superset** of this
doc's kernel ("required"). It contains *both* the required kernel *and*
the optional accelerators — because a functionizer is not self-hostable
(porting an Evident-to-Cranelift compiler to Evident is circular) **and**
not required (an implementor can skip it). So:

```
                       self-hostable?
                    no                    yes
              ┌──────────────────┬──────────────────┐
   required?  │  § 2 KERNEL      │   (empty —        │
        yes   │  front end,      │    if it's        │
              │  solver FFI,     │    required it    │
              │  effect FFI,     │    can't be a     │
              │  scheduler+walk  │    library)       │
              ├──────────────────┼──────────────────┤
   required?  │  bucket (b):     │  bucket (a):      │
        no    │  functionizers,  │  stdlib passes    │
              │  fast FSM tiers  │  (validate, subs, │
              │  (inv. Tier 0)   │  pretty, …)       │
              │                  │  (inv. Tier 1/2)  │
              └──────────────────┴──────────────────┘
                                    (inv. Tier 4 IO:
                                     split — some
                                     kernel scheduler
                                     loop, some
                                     optional commands)
```

The implementor contract **carves the required kernel out of Tier 0** and
declares the rest of Tier 0 (the functionizers) optional. The inventory
tells you what *can become Evident*; this doc tells you what an
implementor *must write at all*. Reading them together: write the
top-left cell; ship the bottom-right cell as Evident; the bottom-left is
yours to write only if you want speed.

---

## § 4 — Tree-walking is a kernel capability, and it is enum-generic

This is the load-bearing architectural decision — the conclusion that
decided the kernel's scope (and session NN's worklist) over building the
tier-2 JIT first. Four claims, in order.

### Self-hosted passes are tree-walks; running one needs an FSM-with-stack

`validate`, `subscriptions`, `pretty` — every interesting stdlib pass —
recurse over a nested `Expr` tree of unknown depth. The constraint
language has **no** recursion: a claim that inlines its own body is
depth-capped at 64 and leaves the inlined frames' outputs *unconstrained*,
so Z3 fills them with garbage that comes back SAT
(`examples/COUNTEREXAMPLES.md` #15; [`../self-hosting.md`](../self-hosting.md)
Gap #1). The supported way to walk a tree without recursion is the
**stack-of-FSMs** ([`loop-functionizer.md`](loop-functionizer.md) § 4):
make the work-stack *explicit data in the FSM's state* and drain it with
the step loop. Pop a node, dispatch on its variant, push its children,
fold its contribution into a threaded accumulator; when the stack drains,
the accumulator is the answer.

The reason this avoids the recursion gap is precise and worth stating:
**the outputs are threaded through FSM state across iterations, never left
free for Z3 to fill** ([`loop-functionizer.md`](loop-functionizer.md)
§ 4, "the stack is a stack of FSMs, not a stack of claims"). Each step is
the *non-recursive* question "given this node and this accumulator, what
are the children and the next accumulator?" — finite, fully constrained,
solvable. The recursion is unrolled by the loop; the partial result lives
in a concrete `Value`, not an unconstrained Z3 variable.

### The capability is enum-generic — it is NOT an AST walker

Here is the subtle, decisive point. The kernel capability is **"run an
FSM-with-stack over a recursive enum to completion with composite
state."** It mentions no `Expr`, no `BodyItem`, no AST. The runtime
traverses an *arbitrary recursive enum* — `enum Tree = Leaf(Int) |
Node(Tree, Tree)`, `enum Stack = Empty | Push(Tree, Stack)`, a user's
`LinkedList`, anything `Cons`/`Nil`-shaped.

The AST is just **one** recursive enum (`stdlib/ast.ev`). The AST walker
is **stdlib** (`stdlib/passes/*.ev`) that *uses* the generic capability.
This is the whole reason self-hosting is possible: the kernel does not
bake in the thing being self-hosted. If the kernel had an AST-specific
walker, then (a) it would not be general, and (b) it would have leaked the
very transform it is supposed to host into the kernel. By making the
capability enum-generic, the AST walk becomes a library that any
recursive-enum traversal — user code included — gets for free.

Session MM proved this concretely: `examples/test_36_sum_tree.ev`
sum-a-tree walks an `enum Tree` via an `enum Stack` work-stack, driven to
halt by tier-3 `run(sum_tree, init)`, with **no AST in sight**
(`examples/COUNTEREXAMPLES.md` #19). The pop/dispatch/push/fold/thread
logic is the same logic `subscriptions::walk_expr` needs; the enum just
happens to be `Tree`, not `Expr`.

### Therefore tree-walking belongs in the kernel, not behind the tier-2 JIT

The stack-of-FSMs abstraction has **three realizations**, one per
nested-FSM tier ([`loop-functionizer.md`](loop-functionizer.md) § 4,
"three realizations") — and only the slowest is the kernel:

| Tier | Where the stack lives | Cost | Bucket |
|---|---|---|---|
| 3 blocking-interpret | FSM **state** (recursive-enum spine), drained by **scheduler ticks** | O(n²) marshal, but correct, **no new machinery** | **kernel** |
| 2 loop-functionizer | native `Vec<Value>` in the loop wrapper | O(n) native push/pop | accelerator (b) |
| 1 symbolic-unroll | symbolic — collapses only for *affine* walks, so ~never for tree-walks | O(1) after compile | accelerator (b) |

Tier 3 *is* the scheduler ([`nested-fsm-strategies.md`](nested-fsm-strategies.md)
§ 2): it adds only "seed from `init`, read the final state" to the
existing tick loop. Its correctness is inherited wholesale —

> *If `F` runs correctly as a top-level FSM, it runs correctly nested.*

So the kernel scheduler, extended to run an FSM-with-stack over a
recursive enum, *already gives* tree-walking. No JIT required.

**Why it cannot be an optimization.** Suppose tree-walking lived only in
the tier-2 loop-functionizer (an accelerator, bucket b). Then an
implementor who skips the accelerator — entirely within their rights under
§ 1 — could not run a single stdlib pass, because every pass is a
tree-walk. Their only recourse would be to **re-implement validate /
subscriptions / pretty in the host language** — which defeats
self-hosting and *raises* implementor burden, the exact opposite of the
two-bucket principle's goal. The contradiction is the proof: the floor
that self-hosted passes stand on must be in the always-correct kernel, or
the passes are not actually free. Tiers 1 and 2 then *accelerate* a
capability that already works — pure speed, exactly as § 1 requires.

This also explains the § 1 caveat: for tree-walks the Z3 solve is **not**
the always-correct floor (it is the recursion gap). The floor is the
scheduler running the walk. That is why this one capability is kernel
while the functionizers — whose floor *is* the Z3 solve — are bucket (b).

### The concrete worklist: finish the kernel scheduler (#19a–d)

Tier 3 today proves the *logic* but does not yet host the *real* walk. The
gap is a short, concrete list of scheduler/translate extensions — the
"finish the kernel" worklist that session NN is closing
(`examples/COUNTEREXAMPLES.md` #19):

- **#19a — `Seq(T)` has no in-step pop/tail/cons.** A constraint body
  can't slice a `Seq` tail or cons onto an opaque `Seq`. *Resolved by
  encoding the stack as a recursive enum cons-list* (`enum Stack = Empty |
  Push(T, Stack)`) where pop is a `match`-destructure and push is a
  constructor call — which is *why* the capability is enum-generic, not
  Seq-based.
- **#19b — nested constructor patterns aren't deep-matched.**
  `Step(Empty, _)` matches any `Step(_, _)`; the recognizer tests only the
  outer constructor. Needs per-level recognizer + field-extraction
  conjoined into the match guard (`translate/exprs/match_expr.rs`).
- **#19c — enum equality against a literal with a nested enum field is
  dropped.** `final = Step(Empty, 6)` doesn't translate; only flat
  single-payload literals (`Done(6)`) do. Needs the enum-literal-equality
  builder to recurse into nested enum-typed args (reuse
  `effect_loop/state.rs::encode_state_value`).
- **#19d — `run`'s `init` can't be a composite.** `run(F, ⟨root⟩)` /
  `run(F, Node(...))` is rejected; `init` must be scalar/given/arith.
  Needs `eval_const_init` + `coerce_init` to accept a pre-built composite
  — *the single most important fix for the `walk_expr` self-host*, since
  that walk seeds the stack with `⟨root_expr⟩` and returns a `Set(String)`
  accumulator (composite in **and** out).

These four are the difference between "the tree-walk pattern is proven
sound on a toy" (session MM) and "the kernel can host the real
`subscriptions::walk_expr`" (the LOC-inverting port,
[`loop-functionizer.md`](loop-functionizer.md) § 5). They are **kernel
scheduler work**, not accelerator work — which is exactly why this doc
classifies them under § 2 piece 4, and why finishing them (NN) is on the
critical path to a conformant minimal runtime, while building tier 2 (OO)
is not.

---

## § 5 — The bootstrap / conformance contract

### What "conformant" means

> A **conformant minimal runtime** = the § 2 kernel + the stdlib it ships.

Nothing more is required. No functionizer, no fast nested-FSM tier, no
host-language pass. The kernel runs the stdlib; the stdlib supplies the
transforms; the corpus runs.

### The bootstrap subtlety, and the one cycle (broken)

The kernel runs the stdlib passes — but the stdlib passes are *processed
by* the kernel. Is that circular? Mirroring
[`self-hosting-inventory.md`](self-hosting-inventory.md)'s bootstrap-chain
analysis: **one structural cycle exists, and the seam breaks it.**

A self-hosted pass (`stdlib/passes/pretty.ev`) is loaded by
`EvidentRuntime` and runs *through* `EvidentRuntime::query`, which is
kernel. The chain is one-directional:

```
user code → rt.pretty(item)        (caller-visible)
          → EvidentPretty.expr(…)   (stdlib pass — bucket a)
          → rt.query("Pretty", …)   (kernel: § 2 piece 2)
          → translate + Z3 solve    (kernel)
          → scheduler / FSM-walk    (kernel: § 2 piece 4)
```

The kernel never reaches back *up* into a pass. The cycle that would be
fatal — **a self-hosted pass the kernel needs in order to load itself** —
does **not** exist and must never be introduced. If `validate.ev` had to
be run by the runtime before `validate.ev` could load, the runtime could
not come up. It can't happen, because a pass is an *optional transform
over user programs*, never a load-time dependency of the kernel: the
kernel parses a pass file (front end, seed), translates its primitive
constraints (solver FFI, seed), and runs it as an FSM-with-stack
(scheduler, seed) using **only the § 2 kernel** — no prior pass required.

So the **seed** is precisely the § 2 kernel: the front end + primitive
lowering + the scheduler-with-tree-walk that can process a pass file
without any pass having run. That is the part that *cannot* be
self-hosted, and it is acyclic with respect to bucket (a). Today's
migration safeguard — "`EvidentRuntime::new()` must succeed with no
Evident pass files on disk; the Rust impl stays the constructible default"
([`../self-hosting.md`](../self-hosting.md);
[`self-hosting-inventory.md`](self-hosting-inventory.md) "bootstrap
fragility") — is the operational form of this rule: the kernel always
comes up on the seed alone.

### The conformance test

A runtime is conformant if it:

1. **Runs the `examples/test_*.ev` corpus** end-to-end — every `sat_*` /
   `unsat_*` claim passes, and every demo runs to its expected exit code
   and output (`evident test examples/` + `cargo test --test demos`, the
   `EXPECTATIONS` contract). This exercises the kernel's four pieces on
   real programs.
2. **Passes the stdlib equivalence tests** — the cross-validation harness
   (`runtime/tests/{validate,subscriptions,pretty}_equivalence.rs`, and
   the nested-FSM `run_fsm.rs` oracle harness) confirms the self-hosted
   passes produce the *same* output as the canonical implementation. For a
   runtime that ships *only* the Evident passes (the minimal end state),
   the equivalence target is the corpus' pinned expected output; for one
   that still carries Rust impls (today), it is byte-identity between the
   two impls.

The corpus is the behavioral spec; the equivalence tests are the
self-hosting spec. A runtime that passes both has demonstrably implemented
the kernel correctly *and* can run the stdlib on it — which is exactly the
definition of conformant above.

---

## § 6 — Open questions

The contract's edges, roughly in order of how much each moves the kernel's
size or shape.

- **How much of `translate/` is irreducible vs self-hostable desugaring?**
  § 2 piece 2 draws the line at "calls the Z3 API" — but the inventory
  already flags `translate/{preprocess, exprs/record_lift, exprs/match_expr,
  inline/rewrite, inline/dispatch}` as partial Tier 1/2 (self-hostable
  AST→AST sitting on top of the primitive lowering). If those migrate to
  stdlib, the constraint-interface kernel shrinks from ~5,000 toward the
  primitive-lowering-only core, and [`minimal-runtime.md`](minimal-runtime.md)'s
  ~8,250 "stage 5" figure becomes the floor. The precise boundary — which
  lowering rules are *primitive* (irreducible) vs *derived* (a desugar a
  pass could express) — is the single largest unknown in the LOC budget.
  It is gated on the same recursive-claim fix that gates the Tier 2 ports
  (`translate/inline/recursion.rs`).

- **Can the parser itself become a self-hosted pass over a seed?**
  [`minimal-runtime.md`](minimal-runtime.md) "parser bootstrapping": a
  minimal Rust seed parser that handles only enough syntax to load
  `stdlib/parser.ev`, which then parses the full language (the Lua /
  Smalltalk / OCaml path). This would cut the front-end kernel from ~2,650
  to a small seed (~6,350 total runtime, "stage 6"). It does not violate
  the bootstrap rule — the seed parser is still kernel, just smaller — but
  it is its own project and is explicitly deferred. The open question is
  whether the seed can be small enough to be worth the indirection.

- **Where exactly is the effect line — kernel FFI vs Evident wrapper?**
  § 2 piece 3 keeps a few built-ins (Print, Read, Time, Exit) plus the
  generic FFI primitive. But Print/Read/Time are arguably *themselves*
  FFI wrappers over `write(2)` / `read(2)` / `clock_gettime` — so the
  irreducible effect kernel might be **just** the FFI primitive + `Exit`
  (the one effect that must touch the scheduler's halt path directly),
  with Print/Read/Time becoming `stdlib/` wrappers like SDL/audio/GL
  already are. That would shrink piece 3 toward ~700. The counter-argument
  is bootstrap convenience: a runtime with no working Print is hard to
  develop against. The line is a judgment call, not a forced one.

- **Does the recursive-enum tree-walk capability need a depth/iteration
  guard in the contract?** § 4 puts the walk in the kernel; a malformed or
  genuinely non-terminating walk must fail *loudly*, not hang
  (`LoopOpts.max_steps` / the loop-functionizer's `max_iters`,
  [`nested-fsm-strategies.md`](nested-fsm-strategies.md) § 2,
  [`loop-functionizer.md`](loop-functionizer.md) § 3). Whether the
  conformance contract should *mandate* a specific guard semantics (so all
  conformant runtimes fail the same way on a non-draining stack), or leave
  it implementation-defined, is open. The nested-recursion-depth case (an
  `F` whose body itself contains `run(G, …)`,
  [`nested-fsm-strategies.md`](nested-fsm-strategies.md) § 8) compounds
  this.

- **Should the contract pin the residual/CEGAR boundary?**
  [`selection-policy.md`](selection-policy.md) § 4 establishes the
  global-freedom rule (a value may be witnessed only when *every*
  constraint on it is locally visible; otherwise defer or solve fully) —
  a *soundness* rule, not a speed one. It lives in the accelerator layer
  today (the satisfier, the future residual functionizer), so it is bucket
  (b) and out of the required kernel. But if any kernel slow path ever
  *witnesses* rather than fully solves, the rule becomes a kernel
  obligation. Worth watching that the kernel's Z3 floor stays a *full*
  solve (which is unconditionally sound) and never quietly becomes a
  witness.
